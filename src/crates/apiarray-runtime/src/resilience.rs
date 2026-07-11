use crate::secret::SecretResolver;
use crate::transport::HttpExecutor;
use crate::{RuntimeError, RuntimeErrorCode};
use apiarray_core::adapter::{AdapterKind, parse_response};
use apiarray_core::canonical::{CanonicalRequest, CanonicalResponse};
use apiarray_core::health::{EndpointHealth, HealthObservation, HealthPolicy};
use apiarray_core::routing::{HealthStatus, StandardError};
use apiarray_core::runtime::{CompiledRuntime, DispatchPlan, DispatchRequest};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::fs::OpenOptions;
use std::future::Future;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

const MAX_TOTAL_ATTEMPTS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailureAction {
    RetrySame,
    TryFailover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptResult {
    Success,
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttemptAudit {
    pub upstream_id: String,
    pub provider_instance: String,
    pub latency_ms: u64,
    pub result: AttemptResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<StandardError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceResult {
    Success,
    Failure,
    ClientDisconnected,
}

/// 不包含请求正文、响应正文、URL、Header 或 Secret 的请求级审计记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_hit: Option<bool>,
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
}

#[derive(Clone)]
pub struct ResilientExecutor {
    transport: HttpExecutor,
    secrets: Arc<dyn SecretResolver>,
    health_policy: HealthPolicy,
    health: Arc<RwLock<BTreeMap<String, EndpointHealth>>>,
    audit: Arc<dyn AuditSink>,
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
        let _ = self.audit.record(trace);
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
                trace.input_tokens = Some(response.usage.input_tokens);
                trace.output_tokens = Some(response.usage.output_tokens);
                trace.cached_input_tokens = response.usage.cached_input_tokens;
                trace.cache_hit = response.usage.cached_input_tokens.map(|tokens| tokens > 0);
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
            let health = self.health_snapshot().await;
            let plan = runtime
                .plan_dispatch(&DispatchRequest {
                    publisher_id: publisher_id.to_owned(),
                    request: request.clone(),
                    health,
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

    async fn health_snapshot(&self) -> BTreeMap<String, HealthStatus> {
        self.health
            .read()
            .await
            .iter()
            .map(|(id, health)| (id.clone(), health.status))
            .collect()
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
        input_tokens: None,
        output_tokens: None,
        cached_input_tokens: None,
        cache_hit: None,
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
                error: None,
            }],
            retry_count: 0,
            failover_count: 0,
            input_tokens: Some(3),
            output_tokens: Some(2),
            cached_input_tokens: Some(1),
            cache_hit: Some(true),
        };
        let value = serde_json::to_value(trace).expect("trace is serializable");
        assert!(value.get("request").is_none());
        assert!(value.get("response").is_none());
        assert!(value.get("url").is_none());
        assert!(value.get("headers").is_none());
    }
}
