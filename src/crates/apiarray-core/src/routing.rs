use crate::{CoreError, ErrorCode, SCHEMA_VERSION, ValidationIssue};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutePolicy {
    pub schema_version: u32,
    pub id: String,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub max_retries: u32,
    #[serde(default)]
    pub failover_on: HashSet<StandardError>,
    #[serde(default)]
    pub selection_strategy: SelectionStrategy,
    #[serde(default = "default_latency_hysteresis")]
    pub latency_hysteresis_ms: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionStrategy {
    #[default]
    PriorityFailover,
    WeightedRoundRobin,
    LowestLatency,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteCandidate {
    pub id: String,
    pub priority: u32,
    #[serde(default = "default_weight")]
    pub weight: u16,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub health: HealthStatus,
    #[serde(default)]
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub models: HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StandardError {
    AuthFailed,
    RateLimited,
    BalanceExhausted,
    ModelUnavailable,
    ProviderTimeout,
    NetworkUnreachable,
    TlsFailed,
    InvalidRequest,
    InvalidResponse,
    ContentRejected,
    InternalRuntimeError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteRequest {
    pub model: String,
    #[serde(default)]
    pub excluded_candidates: HashSet<String>,
    #[serde(default)]
    pub previous_error: Option<StandardError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteDecision {
    pub policy_id: String,
    pub candidate_id: String,
    pub reason: RouteReason,
    pub timeout_ms: u64,
    pub remaining_retries: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteReason {
    PrimaryHealthy,
    PrimaryDegraded,
    FailoverAfterError,
    WeightedSelection,
    LowestLatency,
}

/// Smooth weighted round-robin state. It is runtime-only and is never persisted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WeightedSelectionState {
    current: BTreeMap<String, i64>,
}

impl RoutePolicy {
    /// 根据模型、健康状态、优先级和前序错误选择一个候选线路。
    ///
    /// # Errors
    ///
    /// 策略无效、错误不允许触发切换或没有可用候选线路时返回错误。
    pub fn decide(
        &self,
        request: &RouteRequest,
        candidates: &[RouteCandidate],
    ) -> Result<RouteDecision, CoreError> {
        self.decide_with_state(request, candidates, None)
    }

    /// Selects a route and optionally advances smooth weighted round-robin state.
    /// The supplied state is isolated by the caller per publisher and public model.
    pub fn decide_with_state(
        &self,
        request: &RouteRequest,
        candidates: &[RouteCandidate],
        weighted_state: Option<&mut WeightedSelectionState>,
    ) -> Result<RouteDecision, CoreError> {
        self.validate()?;
        if let Some(error) = request.previous_error
            && !self.failover_on.contains(&error)
        {
            return Err(CoreError::new(
                ErrorCode::RouteUnavailable,
                format!("策略不允许在错误 {error:?} 后切换线路"),
            ));
        }

        let mut eligible: Vec<&RouteCandidate> = candidates
            .iter()
            .filter(|candidate| {
                candidate.enabled
                    && !request.excluded_candidates.contains(&candidate.id)
                    && !matches!(
                        candidate.health,
                        HealthStatus::Unhealthy | HealthStatus::Paused
                    )
                    && (candidate.models.is_empty() || candidate.models.contains(&request.model))
            })
            .collect();
        eligible.sort_by_key(|candidate| {
            (
                health_rank(candidate.health),
                candidate.priority,
                candidate.id.as_str(),
            )
        });
        let best_rank = eligible
            .first()
            .map(|candidate| health_rank(candidate.health));
        let tier = eligible
            .iter()
            .copied()
            .filter(|candidate| Some(health_rank(candidate.health)) == best_rank)
            .collect::<Vec<_>>();
        let selected = match self.selection_strategy {
            SelectionStrategy::PriorityFailover => tier.first().copied(),
            SelectionStrategy::WeightedRoundRobin => weighted_state
                .and_then(|state| smooth_weighted(&tier, state))
                .or_else(|| tier.first().copied()),
            SelectionStrategy::LowestLatency => lowest_latency(&tier, self.latency_hysteresis_ms),
        }
        .ok_or_else(|| {
            CoreError::new(
                ErrorCode::RouteUnavailable,
                format!("没有可用于模型 {} 的候选线路", request.model),
            )
        })?;
        let reason = if request.previous_error.is_some() {
            RouteReason::FailoverAfterError
        } else if self.selection_strategy == SelectionStrategy::WeightedRoundRobin {
            RouteReason::WeightedSelection
        } else if self.selection_strategy == SelectionStrategy::LowestLatency
            && selected.latency_ms.is_some()
        {
            RouteReason::LowestLatency
        } else if selected.health == HealthStatus::Degraded {
            RouteReason::PrimaryDegraded
        } else {
            RouteReason::PrimaryHealthy
        };

        Ok(RouteDecision {
            policy_id: self.id.clone(),
            candidate_id: selected.id.clone(),
            reason,
            timeout_ms: self.timeout_ms,
            remaining_retries: self.max_retries,
        })
    }

    /// 校验策略 Schema、超时和重试范围。
    ///
    /// # Errors
    ///
    /// 策略字段超出 MVP 安全范围时返回错误。
    pub fn validate(&self) -> Result<(), CoreError> {
        let mut issues = Vec::new();
        if self.schema_version != SCHEMA_VERSION {
            issues.push(ValidationIssue::new(
                "schema_version",
                "UNSUPPORTED_SCHEMA",
                "路由策略 Schema 版本不受支持",
            ));
        }
        if self.id.trim().is_empty() {
            issues.push(ValidationIssue::new("id", "REQUIRED", "策略 ID 不能为空"));
        }
        if self.timeout_ms == 0 || self.timeout_ms > 600_000 {
            issues.push(ValidationIssue::new(
                "timeout_ms",
                "OUT_OF_RANGE",
                "超时必须在 1 到 600000 毫秒之间",
            ));
        }
        if self.max_retries > 10 {
            issues.push(ValidationIssue::new(
                "max_retries",
                "OUT_OF_RANGE",
                "首版最大重试次数不能超过 10",
            ));
        }
        if self.latency_hysteresis_ms > 60_000 {
            issues.push(ValidationIssue::new(
                "latency_hysteresis_ms",
                "OUT_OF_RANGE",
                "最低延迟滞回必须在 0 到 60000 毫秒之间",
            ));
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(CoreError::validation(
                ErrorCode::InvalidInput,
                "路由策略校验失败",
                issues,
            ))
        }
    }
}

fn health_rank(status: HealthStatus) -> u8 {
    match status {
        HealthStatus::Healthy => 0,
        HealthStatus::Degraded => 1,
        HealthStatus::Unknown => 2,
        HealthStatus::Unhealthy | HealthStatus::Paused => 3,
    }
}

fn smooth_weighted<'a>(
    candidates: &[&'a RouteCandidate],
    state: &mut WeightedSelectionState,
) -> Option<&'a RouteCandidate> {
    let total = candidates
        .iter()
        .map(|candidate| i64::from(candidate.weight.clamp(1, 100)))
        .sum::<i64>();
    let mut selected: Option<&RouteCandidate> = None;
    let mut selected_score = i64::MIN;
    for candidate in candidates {
        let score = state.current.entry(candidate.id.clone()).or_default();
        *score += i64::from(candidate.weight.clamp(1, 100));
        if *score > selected_score
            || (*score == selected_score
                && selected.is_none_or(|current| candidate.id < current.id))
        {
            selected = Some(*candidate);
            selected_score = *score;
        }
    }
    if let Some(candidate) = selected {
        if let Some(score) = state.current.get_mut(&candidate.id) {
            *score -= total;
        }
    }
    state
        .current
        .retain(|id, _| candidates.iter().any(|candidate| candidate.id == *id));
    selected
}

fn lowest_latency<'a>(
    candidates: &[&'a RouteCandidate],
    hysteresis_ms: u64,
) -> Option<&'a RouteCandidate> {
    let mut sampled = candidates
        .iter()
        .copied()
        .filter(|candidate| candidate.latency_ms.is_some())
        .collect::<Vec<_>>();
    if sampled.is_empty() {
        return candidates.first().copied();
    }
    sampled.sort_by_key(|candidate| {
        (
            candidate.latency_ms.unwrap_or(u64::MAX),
            candidate.priority,
            candidate.id.as_str(),
        )
    });
    let best = sampled[0];
    candidates
        .iter()
        .copied()
        .filter(|candidate| candidate.priority < best.priority)
        .find(|candidate| {
            candidate.latency_ms.is_some_and(|latency| {
                latency
                    <= best
                        .latency_ms
                        .unwrap_or(u64::MAX)
                        .saturating_add(hysteresis_ms)
            })
        })
        .or(Some(best))
}

const fn default_timeout() -> u64 {
    30_000
}

const fn default_true() -> bool {
    true
}

const fn default_weight() -> u16 {
    1
}

const fn default_latency_hysteresis() -> u64 {
    25
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> RoutePolicy {
        RoutePolicy {
            schema_version: 1,
            id: "default".to_owned(),
            timeout_ms: 30_000,
            max_retries: 2,
            failover_on: HashSet::from([StandardError::ProviderTimeout]),
            selection_strategy: SelectionStrategy::PriorityFailover,
            latency_hysteresis_ms: 25,
        }
    }

    #[test]
    fn selects_healthy_candidate_before_degraded() -> Result<(), CoreError> {
        let candidates = vec![
            RouteCandidate {
                id: "degraded-primary".to_owned(),
                priority: 0,
                weight: 1,
                enabled: true,
                health: HealthStatus::Degraded,
                latency_ms: None,
                models: HashSet::new(),
            },
            RouteCandidate {
                id: "healthy-backup".to_owned(),
                priority: 10,
                weight: 1,
                enabled: true,
                health: HealthStatus::Healthy,
                latency_ms: None,
                models: HashSet::new(),
            },
        ];
        let decision = policy().decide(
            &RouteRequest {
                model: "smart".to_owned(),
                excluded_candidates: HashSet::new(),
                previous_error: None,
            },
            &candidates,
        )?;
        assert_eq!(decision.candidate_id, "healthy-backup");
        Ok(())
    }

    #[test]
    fn refuses_failover_for_non_configured_error() {
        let error = policy()
            .decide(
                &RouteRequest {
                    model: "smart".to_owned(),
                    excluded_candidates: HashSet::new(),
                    previous_error: Some(StandardError::AuthFailed),
                },
                &[],
            )
            .expect_err("auth failure must not fail over by default");
        assert_eq!(error.code, ErrorCode::RouteUnavailable);
    }

    #[test]
    fn smooth_weighted_round_robin_preserves_distribution() -> Result<(), CoreError> {
        let mut policy = policy();
        policy.selection_strategy = SelectionStrategy::WeightedRoundRobin;
        let candidates = vec![
            RouteCandidate {
                id: "primary".to_owned(),
                priority: 0,
                weight: 3,
                enabled: true,
                health: HealthStatus::Healthy,
                latency_ms: None,
                models: HashSet::new(),
            },
            RouteCandidate {
                id: "secondary".to_owned(),
                priority: 0,
                weight: 1,
                enabled: true,
                health: HealthStatus::Healthy,
                latency_ms: None,
                models: HashSet::new(),
            },
        ];
        let request = RouteRequest {
            model: "smart".to_owned(),
            excluded_candidates: HashSet::new(),
            previous_error: None,
        };
        let mut state = WeightedSelectionState::default();
        let mut counts = BTreeMap::new();
        for _ in 0..40 {
            let selected = policy.decide_with_state(&request, &candidates, Some(&mut state))?;
            *counts.entry(selected.candidate_id).or_insert(0_u32) += 1;
        }
        assert_eq!(counts["primary"], 30);
        assert_eq!(counts["secondary"], 10);
        Ok(())
    }

    #[test]
    fn lowest_latency_uses_samples_and_priority_hysteresis() -> Result<(), CoreError> {
        let mut policy = policy();
        policy.selection_strategy = SelectionStrategy::LowestLatency;
        policy.latency_hysteresis_ms = 15;
        let request = RouteRequest {
            model: "smart".to_owned(),
            excluded_candidates: HashSet::new(),
            previous_error: None,
        };
        let mut candidates = vec![
            RouteCandidate {
                id: "primary".to_owned(),
                priority: 0,
                weight: 1,
                enabled: true,
                health: HealthStatus::Healthy,
                latency_ms: Some(100),
                models: HashSet::new(),
            },
            RouteCandidate {
                id: "fast".to_owned(),
                priority: 10,
                weight: 1,
                enabled: true,
                health: HealthStatus::Healthy,
                latency_ms: Some(90),
                models: HashSet::new(),
            },
        ];
        assert_eq!(
            policy.decide(&request, &candidates)?.candidate_id,
            "primary"
        );
        candidates[1].latency_ms = Some(70);
        assert_eq!(policy.decide(&request, &candidates)?.candidate_id, "fast");
        Ok(())
    }
}
