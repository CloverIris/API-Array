use crate::routing::StandardError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub schema_version: u32,
    pub correlation_id: String,
    pub publisher_id: String,
    pub route_id: String,
    pub public_model: String,
    pub outcome: AuditOutcome,
    pub total_latency_ms: u64,
    #[serde(default)]
    pub retry_count: u32,
    #[serde(default)]
    pub failover_count: u32,
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
    #[serde(default)]
    pub cache_hit: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AuditOutcome {
    Success,
    Failure { error: StandardError },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthTransition {
    Healthy,
    Flapping,
    Failed,
    FailedOver,
    Recovered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationEvent {
    pub schema_version: u32,
    pub object_id: String,
    pub transition: HealthTransition,
    pub occurrence_count: u32,
    pub summary: String,
}

impl NotificationEvent {
    /// 相同对象和状态可合并，避免按请求刷屏。
    #[must_use]
    pub fn merge(self, newer: Self) -> Option<Self> {
        if self.object_id != newer.object_id || self.transition != newer.transition {
            return None;
        }
        Some(Self {
            occurrence_count: self.occurrence_count.saturating_add(newer.occurrence_count),
            summary: newer.summary,
            ..newer
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_repeated_notification_state() {
        let first = NotificationEvent {
            schema_version: 1,
            object_id: "adapter/a".to_owned(),
            transition: HealthTransition::Failed,
            occurrence_count: 2,
            summary: "failed".to_owned(),
        };
        let newer = NotificationEvent {
            occurrence_count: 3,
            ..first.clone()
        };
        assert_eq!(first.merge(newer).expect("must merge").occurrence_count, 5);
    }
}
