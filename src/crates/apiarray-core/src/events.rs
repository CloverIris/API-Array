use crate::routing::StandardError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationLevel {
    Silent,
    NotificationCenter,
    System,
    ActionRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregatedNotification {
    pub key: String,
    pub event: NotificationEvent,
    pub level: NotificationLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<StandardError>,
    pub first_seen_unix_ms: u64,
    pub last_seen_unix_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NotificationCenter {
    entries: BTreeMap<String, AggregatedNotification>,
}

impl NotificationCenter {
    /// 按对象、状态与错误类型汇聚通知，避免每次请求都制造新消息。
    #[must_use]
    pub fn ingest(
        &mut self,
        event: NotificationEvent,
        error: Option<StandardError>,
        level: NotificationLevel,
        now_unix_ms: u64,
    ) -> AggregatedNotification {
        let key = notification_key(&event, error);
        let Some(entry) = self.entries.get_mut(&key) else {
            let record = AggregatedNotification {
                key: key.clone(),
                event,
                level,
                error,
                first_seen_unix_ms: now_unix_ms,
                last_seen_unix_ms: now_unix_ms,
            };
            self.entries.insert(key, record.clone());
            return record;
        };
        if let Some(merged) = entry.event.clone().merge(event.clone()) {
            entry.event = merged;
        } else {
            entry.event = event;
        }
        entry.level = entry.level.max(level);
        entry.error = error;
        entry.last_seen_unix_ms = now_unix_ms;
        entry.clone()
    }

    #[must_use]
    pub fn records(&self) -> Vec<AggregatedNotification> {
        self.entries.values().cloned().collect()
    }
}

fn notification_key(event: &NotificationEvent, error: Option<StandardError>) -> String {
    format!("{}:{:?}:{error:?}", event.object_id, event.transition)
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

    #[test]
    fn notification_center_aggregates_repeated_failure() {
        let event = NotificationEvent {
            schema_version: 1,
            object_id: "adapter/main".to_owned(),
            transition: HealthTransition::Failed,
            occurrence_count: 1,
            summary: "failed".to_owned(),
        };
        let mut center = NotificationCenter::default();
        let _ = center.ingest(
            event.clone(),
            Some(StandardError::ProviderTimeout),
            NotificationLevel::NotificationCenter,
            10,
        );
        let record = center.ingest(
            event,
            Some(StandardError::ProviderTimeout),
            NotificationLevel::System,
            20,
        );
        assert_eq!(record.event.occurrence_count, 2);
        assert_eq!(record.level, NotificationLevel::System);
        assert_eq!(center.records().len(), 1);
    }
}
