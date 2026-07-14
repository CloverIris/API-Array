use crate::secret::SecretResolver;
use crate::transport::HttpExecutor;
use crate::{RuntimeError, RuntimeErrorCode};
use apiarray_core::adapter::{AdapterKind, parse_response};
use apiarray_core::canonical::{CanonicalRequest, CanonicalResponse, Usage};
use apiarray_core::events::{
    AggregatedNotification, HealthTransition, NotificationEvent, NotificationLevel,
};
use apiarray_core::health::{EndpointHealth, HealthObservation, HealthPolicy};
use apiarray_core::routing::{HealthStatus, RouteReason, SelectionStrategy, StandardError};
use apiarray_core::runtime::{CompiledRuntime, DispatchPlan, DispatchRequest, estimate_usage_cost};
use apiarray_core::workspace::{BillingPolicy, PricingRuleSource};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs::OpenOptions;
use std::future::Future;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

const MAX_TOTAL_ATTEMPTS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailureAction {
    RetrySame,
    TryFailover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptResult {
    Success,
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptAudit {
    pub upstream_id: String,
    pub provider_instance: String,
    pub latency_ms: u64,
    pub result: AttemptResult,
    pub selection_strategy: SelectionStrategy,
    pub route_reason: RouteReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<StandardError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceResult {
    Success,
    Failure,
    ClientDisconnected,
}

/// 不包含请求正文、响应正文、URL、Header 或 Secret 的请求级审计记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionTrace {
    pub schema_version: u32,
    pub correlation_id: String,
    pub publisher_id: String,
    pub public_model: String,
    pub streaming: bool,
    pub started_at_unix_ms: u64,
    pub total_latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_byte_latency_ms: Option<u64>,
    pub result: TraceResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_error: Option<StandardError>,
    pub attempts: Vec<AttemptAudit>,
    pub retry_count: u32,
    pub failover_count: u32,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_strategy: Option<SelectionStrategy>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_upstream_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_hit: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_cost_micros: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing_rule_source: Option<PricingRuleSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing_rule_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monthly_budget_micros: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub budget_warning_thresholds: Vec<u8>,
}

pub trait AuditSink: Send + Sync {
    /// 写入一条已经脱敏的请求级记录。
    ///
    /// # Errors
    ///
    /// 持久化介质不可用时返回安全错误。
    fn record(&self, trace: &ExecutionTrace) -> Result<(), RuntimeError>;
}

#[derive(Debug, Default)]
pub struct NoopAuditSink;

impl AuditSink for NoopAuditSink {
    fn record(&self, _trace: &ExecutionTrace) -> Result<(), RuntimeError> {
        Ok(())
    }
}

pub struct JsonlAuditSink {
    sender: SyncSender<ExecutionTrace>,
}

#[derive(Clone)]
pub struct SqliteAuditSink {
    repository: crate::persistence::WorkspaceRepository,
}

impl SqliteAuditSink {
    #[must_use]
    pub fn new(repository: crate::persistence::WorkspaceRepository) -> Self {
        Self { repository }
    }
}

impl AuditSink for SqliteAuditSink {
    fn record(&self, trace: &ExecutionTrace) -> Result<(), RuntimeError> {
        self.repository.record_audit(trace)?;
        self.record_budget_notifications(trace)
    }
}

impl SqliteAuditSink {
    fn record_budget_notifications(&self, trace: &ExecutionTrace) -> Result<(), RuntimeError> {
        let (Some(cost), Some(budget)) = (trace.estimated_cost_micros, trace.monthly_budget_micros)
        else {
            return Ok(());
        };
        if budget == 0 || trace.budget_warning_thresholds.is_empty() {
            return Ok(());
        }
        let current = self
            .repository
            .estimated_cost_current_month(&trace.publisher_id)?;
        let previous = current.saturating_sub(cost);
        for threshold in &trace.budget_warning_thresholds {
            let boundary = u128::from(budget)
                .saturating_mul(u128::from(*threshold))
                .div_ceil(100);
            let crossed = u128::from(previous) < boundary && u128::from(current) >= boundary;
            if !crossed {
                continue;
            }
            let now = trace
                .started_at_unix_ms
                .saturating_add(trace.total_latency_ms);
            let level = if *threshold >= 100 {
                NotificationLevel::System
            } else {
                NotificationLevel::NotificationCenter
            };
            let notification = AggregatedNotification {
                key: format!("budget:{}:{}", trace.publisher_id, threshold),
                event: NotificationEvent {
                    schema_version: 1,
                    object_id: trace.publisher_id.clone(),
                    transition: HealthTransition::BudgetWarning,
                    occurrence_count: 1,
                    summary: format!(
                        "入口本月估算用量已达到预算的 {}%（仅为估算，请以供应商账单为准）",
                        threshold
                    ),
                },
                level,
                error: None,
                first_seen_unix_ms: now,
                last_seen_unix_ms: now,
            };
            self.repository.upsert_notification(&notification)?;
        }
        Ok(())
    }
}

impl JsonlAuditSink {
    /// 打开只追加的本地 JSONL 审计文件。
    ///
    /// # Errors
    ///
    /// 文件无法创建或打开时返回安全错误。
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RuntimeError> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|_| {
                RuntimeError::new(RuntimeErrorCode::AuditUnavailable, "无法打开本地审计文件")
            })?;
        let (sender, receiver) = sync_channel::<ExecutionTrace>(1_024);
        std::thread::Builder::new()
            .name("apiarray-audit".to_owned())
            .spawn(move || {
                let mut writer = BufWriter::new(file);
                while let Ok(trace) = receiver.recv() {
                    if serde_json::to_writer(&mut writer, &trace).is_err()
                        || writer.write_all(b"\n").is_err()
                        || writer.flush().is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|_| {
                RuntimeError::new(RuntimeErrorCode::AuditUnavailable, "无法启动本地审计线程")
            })?;
        Ok(Self { sender })
    }
}

impl AuditSink for JsonlAuditSink {
    fn record(&self, trace: &ExecutionTrace) -> Result<(), RuntimeError> {
        self.sender.try_send(trace.clone()).map_err(|error| {
            let message = match error {
                TrySendError::Full(_) => "本地审计队列已满",
                TrySendError::Disconnected(_) => "本地审计线程已停止",
            };
            RuntimeError::new(RuntimeErrorCode::AuditUnavailable, message)
        })
    }
}

/// Reads the most recent local audit records. Invalid or interrupted lines are
/// ignored so a partial write never makes the audit timeline unavailable.
pub fn read_jsonl_audit(
    path: impl AsRef<Path>,
    limit: usize,
) -> Result<Vec<ExecutionTrace>, RuntimeError> {
    let path = path.as_ref();
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let file = std::fs::File::open(path).map_err(|_| {
        RuntimeError::new(RuntimeErrorCode::AuditUnavailable, "无法读取本地审计文件")
    })?;
    let mut records = BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<ExecutionTrace>(&line).ok())
        .collect::<Vec<_>>();
    records.sort_by_key(|record| std::cmp::Reverse(record.started_at_unix_ms));
    records.truncate(limit.clamp(1, 1_000));
    Ok(records)
}

pub struct ExecutedJson {
    pub response: CanonicalResponse,
    pub public_model: String,
    pub trace: ExecutionTrace,
}

pub struct ExecutedStream {
    pub response: reqwest::Response,
    pub adapter: AdapterKind,
    pub public_model: String,
    pub trace: ExecutionTrace,
    pub billing: BillingPolicy,
    pub budget_warning_thresholds: Vec<u8>,
    pub pricing_model: String,
}

/// Applies provider-reported usage and a versioned wallet pricing rule to an
/// audit trace. This contains no request or response content.
pub fn apply_usage_accounting(
    trace: &mut ExecutionTrace,
    usage: &Usage,
    billing: &BillingPolicy,
    warning_thresholds: &[u8],
    pricing_model: &str,
) {
    trace.input_tokens = Some(usage.input_tokens);
    trace.output_tokens = Some(usage.output_tokens);
    trace.cached_input_tokens = usage.cached_input_tokens;
    trace.cache_hit = usage.cached_input_tokens.map(|tokens| tokens > 0);
    trace.monthly_budget_micros = billing.monthly_budget_micros;
    trace.budget_warning_thresholds = warning_thresholds.to_vec();
    if let Some(estimate) = estimate_usage_cost(billing, pricing_model, usage) {
        trace.estimated_cost_micros = Some(estimate.estimated_cost_micros);
        trace.cost_currency = estimate.currency;
        trace.pricing_rule_source = Some(estimate.rule_source);
        trace.pricing_rule_version = Some(estimate.rule_version);
        trace.pricing_model = Some(pricing_model.to_owned());
    }
}

#[derive(Clone)]
pub struct ResilientExecutor {
    transport: HttpExecutor,
    secrets: Arc<dyn SecretResolver>,
    health_policy: HealthPolicy,
    health: Arc<RwLock<BTreeMap<String, EndpointHealth>>>,
    audit: Arc<dyn AuditSink>,
    audit_healthy: Arc<AtomicBool>,
}

impl ResilientExecutor {
    #[must_use]
    pub fn new(
        transport: HttpExecutor,
        secrets: Arc<dyn SecretResolver>,
        audit: Arc<dyn AuditSink>,
    ) -> Self {
        Self {
            transport,
            secrets,
            health_policy: HealthPolicy::default(),
            health: Arc::new(RwLock::new(BTreeMap::new())),
            audit,
            audit_healthy: Arc::new(AtomicBool::new(true)),
        }
    }

    pub async fn set_health(&self, upstream_id: impl Into<String>, status: HealthStatus) {
        let mut endpoint = EndpointHealth::unknown();
        endpoint.status = status;
        self.health
            .write()
            .await
            .insert(upstream_id.into(), endpoint);
    }

    pub fn record(&self, trace: &ExecutionTrace) {
        if self.audit.record(trace).is_err() {
            self.audit_healthy.store(false, AtomicOrdering::Release);
        }
    }

    #[must_use]
    pub fn audit_healthy(&self) -> bool {
        self.audit_healthy.load(AtomicOrdering::Acquire)
    }

    /// 执行带重试、切换、健康回写和审计的非流式请求。
    ///
    /// # Errors
    ///
    /// 路由不可用、Secret 缺失或全部上游尝试失败时返回错误。
    pub async fn execute_json(
        &self,
        runtime: &CompiledRuntime,
        publisher_id: &str,
        request: CanonicalRequest,
        correlation_id: String,
    ) -> Result<ExecutedJson, RuntimeError> {
        let started = Instant::now();
        let started_at = unix_millis();
        let mut attempts = Vec::new();
        let result = self
            .execute_with_retry(
                runtime,
                publisher_id,
                &request,
                &mut attempts,
                |plan| async move {
                    let value = self
                        .transport
                        .execute_json(&plan.transport, self.secrets.as_ref())
                        .await?;
                    parse_response(plan.transport.adapter, &value).map_err(|_| {
                        RuntimeError::upstream(
                            StandardError::InvalidResponse,
                            "上游响应不符合 Provider 协议",
                        )
                    })
                },
            )
            .await;
        match result {
            Ok((response, plan)) => {
                let mut trace = make_trace(
                    correlation_id,
                    publisher_id,
                    &request.model,
                    false,
                    started_at,
                    started.elapsed(),
                    TraceResult::Success,
                    None,
                    attempts,
                );
                apply_usage_accounting(
                    &mut trace,
                    &response.usage,
                    &plan.billing,
                    &plan.budget_warning_thresholds,
                    &plan.upstream_model,
                );
                self.record(&trace);
                Ok(ExecutedJson {
                    response,
                    public_model: plan.public_model,
                    trace,
                })
            }
            Err(error) => {
                let trace = make_trace(
                    correlation_id,
                    publisher_id,
                    &request.model,
                    false,
                    started_at,
                    started.elapsed(),
                    TraceResult::Failure,
                    standard_error(&error),
                    attempts,
                );
                self.record(&trace);
                Err(error)
            }
        }
    }

    /// 执行到成功取得流式响应头；流的最终审计由消费方完成。
    ///
    /// # Errors
    ///
    /// 路由不可用、Secret 缺失或全部上游尝试失败时返回错误。
    pub async fn execute_stream(
        &self,
        runtime: &CompiledRuntime,
        publisher_id: &str,
        request: CanonicalRequest,
        correlation_id: String,
    ) -> Result<ExecutedStream, RuntimeError> {
        let started = Instant::now();
        let started_at = unix_millis();
        let mut attempts = Vec::new();
        let result = self
            .execute_with_retry(
                runtime,
                publisher_id,
                &request,
                &mut attempts,
                |plan| async move {
                    self.transport
                        .execute_stream(&plan.transport, self.secrets.as_ref())
                        .await
                },
            )
            .await;
        match result {
            Ok((response, plan)) => Ok(ExecutedStream {
                response,
                adapter: plan.transport.adapter,
                public_model: plan.public_model,
                trace: make_trace(
                    correlation_id,
                    publisher_id,
                    &request.model,
                    true,
                    started_at,
                    started.elapsed(),
                    TraceResult::Success,
                    None,
                    attempts,
                ),
                billing: plan.billing,
                budget_warning_thresholds: plan.budget_warning_thresholds,
                pricing_model: plan.upstream_model,
            }),
            Err(error) => {
                let trace = make_trace(
                    correlation_id,
                    publisher_id,
                    &request.model,
                    true,
                    started_at,
                    started.elapsed(),
                    TraceResult::Failure,
                    standard_error(&error),
                    attempts,
                );
                self.record(&trace);
                Err(error)
            }
        }
    }

    async fn execute_with_retry<T, F, Fut>(
        &self,
        runtime: &CompiledRuntime,
        publisher_id: &str,
        request: &CanonicalRequest,
        attempts: &mut Vec<AttemptAudit>,
        mut execute: F,
    ) -> Result<(T, DispatchPlan), RuntimeError>
    where
        F: FnMut(DispatchPlan) -> Fut,
        Fut: Future<Output = Result<T, RuntimeError>>,
    {
        let mut excluded = HashSet::new();
        let mut previous_error = None;
        let mut last_error = None;
        loop {
            let (health, latency_ms) = self.health_snapshot().await;
            let plan = runtime
                .plan_dispatch(&DispatchRequest {
                    publisher_id: publisher_id.to_owned(),
                    request: request.clone(),
                    health,
                    latency_ms,
                    excluded_upstreams: excluded.clone(),
                    previous_error,
                })
                .map_err(|error| last_error.clone().unwrap_or_else(|| error.into()))?;
            for retry in 0..=plan.route.remaining_retries {
                if attempts.len() >= MAX_TOTAL_ATTEMPTS {
                    return Err(last_error.unwrap_or_else(|| {
                        RuntimeError::upstream(
                            StandardError::InternalRuntimeError,
                            "请求超过最大尝试次数",
                        )
                    }));
                }
                let attempt_started = Instant::now();
                match execute(plan.clone()).await {
                    Ok(value) => {
                        let latency = duration_millis(attempt_started.elapsed());
                        attempts.push(attempt(&plan, latency, None));
                        self.observe(&plan.route.candidate_id, true, latency, None)
                            .await;
                        return Ok((value, plan));
                    }
                    Err(error) => {
                        let latency = duration_millis(attempt_started.elapsed());
                        let normalized =
                            standard_error(&error).unwrap_or(StandardError::InternalRuntimeError);
                        attempts.push(attempt(&plan, latency, Some(normalized)));
                        self.observe(&plan.route.candidate_id, false, latency, Some(normalized))
                            .await;
                        let action =
                            failure_action(retry, plan.route.remaining_retries, normalized);
                        last_error = Some(error);
                        previous_error = Some(normalized);
                        if action == FailureAction::RetrySame {
                            continue;
                        }
                        excluded.insert(plan.route.candidate_id.clone());
                        break;
                    }
                }
            }
        }
    }

    async fn health_snapshot(&self) -> (BTreeMap<String, HealthStatus>, BTreeMap<String, u64>) {
        let health = self.health.read().await;
        (
            health
                .iter()
                .map(|(id, item)| (id.clone(), item.status))
                .collect(),
            health
                .iter()
                .filter_map(|(id, item)| item.ewma_latency_ms.map(|latency| (id.clone(), latency)))
                .collect(),
        )
    }

    async fn observe(
        &self,
        upstream_id: &str,
        success: bool,
        latency_ms: u64,
        error: Option<StandardError>,
    ) {
        let mut health = self.health.write().await;
        let endpoint = health
            .entry(upstream_id.to_owned())
            .or_insert_with(EndpointHealth::unknown);
        let _ = endpoint.observe(
            &self.health_policy,
            HealthObservation {
                success,
                latency_ms: Some(latency_ms),
                error,
            },
        );
    }
}

fn attempt(plan: &DispatchPlan, latency_ms: u64, error: Option<StandardError>) -> AttemptAudit {
    AttemptAudit {
        upstream_id: plan.route.candidate_id.clone(),
        provider_instance: plan.provider_instance.clone(),
        latency_ms,
        result: if error.is_some() {
            AttemptResult::Failure
        } else {
            AttemptResult::Success
        },
        selection_strategy: plan.selection_strategy,
        route_reason: plan.route.reason,
        error,
    }
}

#[allow(clippy::too_many_arguments)]
fn make_trace(
    correlation_id: String,
    publisher_id: &str,
    public_model: &str,
    streaming: bool,
    started_at_unix_ms: u64,
    elapsed: std::time::Duration,
    result: TraceResult,
    final_error: Option<StandardError>,
    attempts: Vec<AttemptAudit>,
) -> ExecutionTrace {
    let first_byte_latency_ms = streaming
        .then(|| attempts.last().map(|attempt| attempt.latency_ms))
        .flatten();
    let unique_upstreams = attempts
        .iter()
        .map(|attempt| attempt.upstream_id.as_str())
        .collect::<HashSet<_>>()
        .len();
    let failover_count = u32::try_from(unique_upstreams.saturating_sub(1)).unwrap_or(u32::MAX);
    let retry_count =
        u32::try_from(attempts.len().saturating_sub(unique_upstreams)).unwrap_or(u32::MAX);
    let selection_strategy = attempts.last().map(|attempt| attempt.selection_strategy);
    let final_upstream_id = attempts.last().map(|attempt| attempt.upstream_id.clone());
    ExecutionTrace {
        schema_version: 1,
        correlation_id,
        publisher_id: publisher_id.to_owned(),
        public_model: public_model.to_owned(),
        streaming,
        started_at_unix_ms,
        total_latency_ms: duration_millis(elapsed),
        first_byte_latency_ms,
        result,
        final_error,
        attempts,
        retry_count,
        failover_count,
        selection_strategy,
        final_upstream_id,
        input_tokens: None,
        output_tokens: None,
        cached_input_tokens: None,
        cache_hit: None,
        estimated_cost_micros: None,
        cost_currency: None,
        pricing_rule_source: None,
        pricing_rule_version: None,
        pricing_model: None,
        monthly_budget_micros: None,
        budget_warning_thresholds: Vec::new(),
    }
}

fn standard_error(error: &RuntimeError) -> Option<StandardError> {
    error.upstream_error.or(match error.code {
        RuntimeErrorCode::ResponseTooLarge | RuntimeErrorCode::ResponseInvalid => {
            Some(StandardError::InvalidResponse)
        }
        _ => None,
    })
}

const fn is_retryable(error: StandardError) -> bool {
    matches!(
        error,
        StandardError::RateLimited
            | StandardError::ProviderTimeout
            | StandardError::NetworkUnreachable
            | StandardError::InvalidResponse
    )
}

const fn failure_action(retry_index: u32, max_retries: u32, error: StandardError) -> FailureAction {
    if is_retryable(error) && retry_index < max_retries {
        FailureAction::RetrySame
    } else {
        FailureAction::TryFailover
    }
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, duration_millis)
}

fn duration_millis(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::{MemorySecretStore, StoreSecretResolver};

    struct FailingAuditSink;

    impl AuditSink for FailingAuditSink {
        fn record(&self, _trace: &ExecutionTrace) -> Result<(), RuntimeError> {
            Err(RuntimeError::new(
                RuntimeErrorCode::AuditUnavailable,
                "audit fixture unavailable",
            ))
        }
    }

    fn audit_trace() -> ExecutionTrace {
        ExecutionTrace {
            schema_version: 1,
            correlation_id: "req-audit-health".to_owned(),
            publisher_id: "publisher-a".to_owned(),
            public_model: "default".to_owned(),
            streaming: false,
            started_at_unix_ms: 1,
            total_latency_ms: 1,
            first_byte_latency_ms: None,
            result: TraceResult::Success,
            final_error: None,
            attempts: Vec::new(),
            retry_count: 0,
            failover_count: 0,
            selection_strategy: None,
            final_upstream_id: None,
            input_tokens: None,
            output_tokens: None,
            cached_input_tokens: None,
            cache_hit: None,
            estimated_cost_micros: None,
            cost_currency: None,
            pricing_rule_source: None,
            pricing_rule_version: None,
            pricing_model: None,
            monthly_budget_micros: None,
            budget_warning_thresholds: Vec::new(),
        }
    }

    #[test]
    fn audit_failure_permanently_degrades_the_executor_snapshot() -> Result<(), RuntimeError> {
        let store = Arc::new(MemorySecretStore::default());
        let resolver = Arc::new(StoreSecretResolver::new(store));
        let executor = ResilientExecutor::new(
            HttpExecutor::new(crate::transport::TransportConfig::default())?,
            resolver,
            Arc::new(FailingAuditSink),
        );
        assert!(executor.audit_healthy());
        executor.record(&audit_trace());
        assert!(!executor.audit_healthy());
        Ok(())
    }

    #[test]
    fn retry_classification_is_conservative() {
        assert!(is_retryable(StandardError::RateLimited));
        assert!(is_retryable(StandardError::ProviderTimeout));
        assert!(!is_retryable(StandardError::AuthFailed));
        assert!(!is_retryable(StandardError::InvalidRequest));
    }

    #[test]
    fn failure_action_retries_then_fails_over() {
        assert_eq!(
            failure_action(0, 2, StandardError::ProviderTimeout),
            FailureAction::RetrySame
        );
        assert_eq!(
            failure_action(2, 2, StandardError::ProviderTimeout),
            FailureAction::TryFailover
        );
        assert_eq!(
            failure_action(0, 2, StandardError::AuthFailed),
            FailureAction::TryFailover
        );
    }

    #[test]
    fn sqlite_audit_emits_budget_notification_only_when_threshold_is_crossed() {
        let root = std::env::temp_dir().join(format!(
            "apiarray-budget-audit-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        let repository = crate::persistence::WorkspaceRepository::new(&root);
        let sink = SqliteAuditSink::new(repository.clone());
        let mut trace = audit_trace();
        trace.publisher_id = "publisher-budget".to_owned();
        trace.started_at_unix_ms = unix_millis();
        trace.estimated_cost_micros = Some(30);
        trace.monthly_budget_micros = Some(100);
        trace.budget_warning_thresholds = vec![50];
        sink.record(&trace).expect("first audit");
        assert!(
            repository
                .read_notifications(crate::persistence::NotificationQuery::default())
                .expect("notifications")
                .is_empty()
        );
        trace.started_at_unix_ms = trace.started_at_unix_ms.saturating_add(1);
        sink.record(&trace).expect("second audit");
        let notifications = repository
            .read_notifications(crate::persistence::NotificationQuery::default())
            .expect("notifications");
        assert_eq!(notifications.len(), 1);
        assert!(notifications[0].summary.contains("50%"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn jsonl_audit_never_serializes_content_or_transport_data() {
        let trace = ExecutionTrace {
            schema_version: 1,
            correlation_id: "req-1".to_owned(),
            publisher_id: "local".to_owned(),
            public_model: "smart".to_owned(),
            streaming: false,
            started_at_unix_ms: 1,
            total_latency_ms: 2,
            first_byte_latency_ms: None,
            result: TraceResult::Success,
            final_error: None,
            attempts: vec![AttemptAudit {
                upstream_id: "primary".to_owned(),
                provider_instance: "provider".to_owned(),
                latency_ms: 2,
                result: AttemptResult::Success,
                selection_strategy: SelectionStrategy::PriorityFailover,
                route_reason: RouteReason::PrimaryHealthy,
                error: None,
            }],
            retry_count: 0,
            failover_count: 0,
            selection_strategy: Some(SelectionStrategy::PriorityFailover),
            final_upstream_id: Some("primary".to_owned()),
            input_tokens: Some(3),
            output_tokens: Some(2),
            cached_input_tokens: Some(1),
            cache_hit: Some(true),
            estimated_cost_micros: None,
            cost_currency: None,
            pricing_rule_source: None,
            pricing_rule_version: None,
            pricing_model: None,
            monthly_budget_micros: None,
            budget_warning_thresholds: Vec::new(),
        };
        let value = serde_json::to_value(trace).expect("trace is serializable");
        assert!(value.get("request").is_none());
        assert!(value.get("response").is_none());
        assert!(value.get("url").is_none());
        assert!(value.get("headers").is_none());
    }

    #[test]
    fn reads_newest_audit_records_without_accepting_invalid_lines() {
        let path = std::env::temp_dir().join(format!("apiarray-audit-{}.jsonl", unix_millis()));
        let trace = ExecutionTrace {
            schema_version: 1,
            correlation_id: "req-audit".to_owned(),
            publisher_id: "local".to_owned(),
            public_model: "smart".to_owned(),
            streaming: false,
            started_at_unix_ms: 10,
            total_latency_ms: 20,
            first_byte_latency_ms: None,
            result: TraceResult::Success,
            final_error: None,
            attempts: Vec::new(),
            retry_count: 0,
            failover_count: 0,
            selection_strategy: None,
            final_upstream_id: None,
            input_tokens: None,
            output_tokens: None,
            cached_input_tokens: None,
            cache_hit: None,
            estimated_cost_micros: None,
            cost_currency: None,
            pricing_rule_source: None,
            pricing_rule_version: None,
            pricing_model: None,
            monthly_budget_micros: None,
            budget_warning_thresholds: Vec::new(),
        };
        std::fs::write(
            &path,
            format!(
                "not-json\n{}\n",
                serde_json::to_string(&trace).expect("trace json")
            ),
        )
        .expect("audit fixture");
        let records = read_jsonl_audit(&path, 10).expect("records");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].correlation_id, "req-audit");
        let _ = std::fs::remove_file(path);
    }
}
