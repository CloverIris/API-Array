use crate::{CoreError, ErrorCode, SCHEMA_VERSION, ValidationIssue};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteCandidate {
    pub id: String,
    pub priority: u32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub health: HealthStatus,
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
            let health_rank = match candidate.health {
                HealthStatus::Healthy => 0,
                HealthStatus::Degraded => 1,
                HealthStatus::Unknown => 2,
                HealthStatus::Unhealthy | HealthStatus::Paused => 3,
            };
            (health_rank, candidate.priority, candidate.id.as_str())
        });

        let selected = eligible.first().ok_or_else(|| {
            CoreError::new(
                ErrorCode::RouteUnavailable,
                format!("没有可用于模型 {} 的候选线路", request.model),
            )
        })?;
        let reason = if request.previous_error.is_some() {
            RouteReason::FailoverAfterError
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

const fn default_timeout() -> u64 {
    30_000
}

const fn default_true() -> bool {
    true
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
        }
    }

    #[test]
    fn selects_healthy_candidate_before_degraded() -> Result<(), CoreError> {
        let candidates = vec![
            RouteCandidate {
                id: "degraded-primary".to_owned(),
                priority: 0,
                enabled: true,
                health: HealthStatus::Degraded,
                models: HashSet::new(),
            },
            RouteCandidate {
                id: "healthy-backup".to_owned(),
                priority: 10,
                enabled: true,
                health: HealthStatus::Healthy,
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
}
