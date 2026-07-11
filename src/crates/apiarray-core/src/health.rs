use crate::routing::{HealthStatus, StandardError};
use crate::{CoreError, ErrorCode, SCHEMA_VERSION, ValidationIssue};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthPolicy {
    pub schema_version: u32,
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,
    #[serde(default = "default_recovery_threshold")]
    pub recovery_threshold: u32,
    #[serde(default = "default_degraded_latency")]
    pub degraded_latency_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthObservation {
    pub success: bool,
    #[serde(default)]
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub error: Option<StandardError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointHealth {
    pub status: HealthStatus,
    pub consecutive_successes: u32,
    pub consecutive_failures: u32,
    #[serde(default)]
    pub last_latency_ms: Option<u64>,
    #[serde(default)]
    pub last_error: Option<StandardError>,
    pub observation_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthChange {
    pub previous: HealthStatus,
    pub current: HealthStatus,
    pub observation_count: u64,
}

impl HealthPolicy {
    /// 校验健康状态阈值。
    ///
    /// # Errors
    ///
    /// Schema 不受支持、阈值为零或延迟阈值超出十分钟时返回错误。
    pub fn validate(&self) -> Result<(), CoreError> {
        let mut issues = Vec::new();
        if self.schema_version != SCHEMA_VERSION {
            issues.push(ValidationIssue::new(
                "schema_version",
                "UNSUPPORTED_SCHEMA",
                "健康策略 Schema 版本不受支持",
            ));
        }
        if self.failure_threshold == 0 || self.failure_threshold > 100 {
            issues.push(ValidationIssue::new(
                "failure_threshold",
                "OUT_OF_RANGE",
                "失败阈值必须在 1 到 100 之间",
            ));
        }
        if self.recovery_threshold == 0 || self.recovery_threshold > 100 {
            issues.push(ValidationIssue::new(
                "recovery_threshold",
                "OUT_OF_RANGE",
                "恢复阈值必须在 1 到 100 之间",
            ));
        }
        if self.degraded_latency_ms == 0 || self.degraded_latency_ms > 600_000 {
            issues.push(ValidationIssue::new(
                "degraded_latency_ms",
                "OUT_OF_RANGE",
                "降级延迟阈值必须在 1 到 600000 毫秒之间",
            ));
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(CoreError::validation(
                ErrorCode::RuntimeConfigInvalid,
                "健康策略校验失败",
                issues,
            ))
        }
    }
}

impl Default for HealthPolicy {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            failure_threshold: default_failure_threshold(),
            recovery_threshold: default_recovery_threshold(),
            degraded_latency_ms: default_degraded_latency(),
        }
    }
}

impl EndpointHealth {
    #[must_use]
    pub const fn unknown() -> Self {
        Self {
            status: HealthStatus::Unknown,
            consecutive_successes: 0,
            consecutive_failures: 0,
            last_latency_ms: None,
            last_error: None,
            observation_count: 0,
        }
    }

    /// 应用一次真实请求或健康探测结果，返回可用于通知聚合的状态变化。
    ///
    /// # Errors
    ///
    /// 策略无效，或失败观察没有标准错误时返回错误。
    pub fn observe(
        &mut self,
        policy: &HealthPolicy,
        observation: HealthObservation,
    ) -> Result<Option<HealthChange>, CoreError> {
        policy.validate()?;
        if !observation.success && observation.error.is_none() {
            return Err(CoreError::new(
                ErrorCode::InvalidInput,
                "失败的健康观察必须包含标准错误",
            ));
        }

        let previous = self.status;
        self.observation_count = self.observation_count.saturating_add(1);
        self.last_latency_ms = observation.latency_ms;
        if observation.success {
            self.consecutive_successes = self.consecutive_successes.saturating_add(1);
            self.consecutive_failures = 0;
            self.last_error = None;
            let slow = observation
                .latency_ms
                .is_some_and(|latency| latency >= policy.degraded_latency_ms);
            self.status = if slow
                || (previous == HealthStatus::Unhealthy
                    && self.consecutive_successes < policy.recovery_threshold)
            {
                HealthStatus::Degraded
            } else {
                HealthStatus::Healthy
            };
        } else {
            self.consecutive_failures = self.consecutive_failures.saturating_add(1);
            self.consecutive_successes = 0;
            self.last_error = observation.error;
            self.status = if self.consecutive_failures >= policy.failure_threshold {
                HealthStatus::Unhealthy
            } else {
                HealthStatus::Degraded
            };
        }

        Ok((previous != self.status).then_some(HealthChange {
            previous,
            current: self.status,
            observation_count: self.observation_count,
        }))
    }
}

const fn default_failure_threshold() -> u32 {
    3
}

const fn default_recovery_threshold() -> u32 {
    2
}

const fn default_degraded_latency() -> u64 {
    10_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitions_through_degraded_unhealthy_and_recovery() -> Result<(), CoreError> {
        let policy = HealthPolicy {
            failure_threshold: 2,
            recovery_threshold: 2,
            ..HealthPolicy::default()
        };
        let mut health = EndpointHealth::unknown();
        health.observe(
            &policy,
            HealthObservation {
                success: false,
                latency_ms: None,
                error: Some(StandardError::ProviderTimeout),
            },
        )?;
        assert_eq!(health.status, HealthStatus::Degraded);
        health.observe(
            &policy,
            HealthObservation {
                success: false,
                latency_ms: None,
                error: Some(StandardError::ProviderTimeout),
            },
        )?;
        assert_eq!(health.status, HealthStatus::Unhealthy);
        health.observe(
            &policy,
            HealthObservation {
                success: true,
                latency_ms: Some(20),
                error: None,
            },
        )?;
        assert_eq!(health.status, HealthStatus::Degraded);
        health.observe(
            &policy,
            HealthObservation {
                success: true,
                latency_ms: Some(20),
                error: None,
            },
        )?;
        assert_eq!(health.status, HealthStatus::Healthy);
        Ok(())
    }

    #[test]
    fn slow_success_is_degraded() -> Result<(), CoreError> {
        let policy = HealthPolicy::default();
        let mut health = EndpointHealth::unknown();
        health.observe(
            &policy,
            HealthObservation {
                success: true,
                latency_ms: Some(policy.degraded_latency_ms),
                error: None,
            },
        )?;
        assert_eq!(health.status, HealthStatus::Degraded);
        Ok(())
    }
}
