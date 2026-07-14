//! One loopback listener mounting independently authenticated publisher routes.

use crate::{RuntimeError, RuntimeErrorCode, publisher::PublisherState};
use apiarray_core::routing::HealthStatus;
use axum::Router;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, net::SocketAddr};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

pub struct GatewayEntry {
    pub prefix: String,
    pub publisher_id: String,
    pub state: PublisherState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayEntryStatus {
    pub prefix: String,
    pub publisher_id: String,
    pub mounted: bool,
    pub blocked: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayStatus {
    pub configured_address: String,
    pub configured_port: u16,
    pub bound_address: Option<String>,
    pub bound_port: Option<u16>,
    pub running: bool,
    pub entries: Vec<GatewayEntryStatus>,
    pub blocked_entries: Vec<GatewayEntryStatus>,
    pub duplicate_routes: Vec<String>,
    pub error: Option<String>,
}

pub struct LocalGateway {
    address: SocketAddr,
    configuration_revision: u64,
    entries: Vec<GatewayEntry>,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl LocalGateway {
    pub async fn start(
        address: SocketAddr,
        entries: Vec<GatewayEntry>,
    ) -> Result<Self, RuntimeError> {
        Self::start_with_revision(address, entries, 0).await
    }

    pub async fn start_with_revision(
        address: SocketAddr,
        entries: Vec<GatewayEntry>,
        configuration_revision: u64,
    ) -> Result<Self, RuntimeError> {
        if !address.ip().is_loopback() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::PublisherBindFailed,
                "gateway must bind to loopback",
            ));
        }

        Self::validate_entries(&entries)?;

        let listener = TcpListener::bind(address).await.map_err(|_| {
            RuntimeError::new(
                RuntimeErrorCode::PublisherBindFailed,
                format!("unable to bind gateway {address}"),
            )
        })?;
        let address = listener.local_addr().map_err(|_| {
            RuntimeError::new(
                RuntimeErrorCode::PublisherBindFailed,
                "unable to read gateway address",
            )
        })?;
        let mut router = Router::new();
        for entry in &entries {
            router = router.nest(&entry.prefix, entry.state.clone().router());
        }
        let (shutdown, receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = receiver.await;
                })
                .await;
        });
        Ok(Self {
            address,
            configuration_revision,
            entries,
            shutdown: Some(shutdown),
            task: Some(task),
        })
    }

    /// Validate a full gateway mount table before a caller touches the active listener.
    ///
    /// The desktop host uses this as a candidate-gateway preflight step: route
    /// mistakes must be rejected while the previous listener is still serving.
    pub fn validate_entries(entries: &[GatewayEntry]) -> Result<(), RuntimeError> {
        Self::validate_entry_prefixes(entries.iter().map(|entry| entry.prefix.as_str()))
    }

    /// Validate prefixes without requiring `PublisherState`; this keeps route
    /// invariants cheap to unit-test and reusable by diagnostics.
    pub fn validate_entry_prefixes<'a>(
        prefixes: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), RuntimeError> {
        let mut seen = HashSet::new();
        for prefix in prefixes {
            if !prefix.starts_with('/') || prefix.contains("..") || prefix.ends_with('/') {
                return Err(RuntimeError::new(
                    RuntimeErrorCode::CoreRejected,
                    "invalid gateway entry prefix",
                ));
            }
            if !seen.insert(prefix.to_owned()) {
                return Err(RuntimeError::new(
                    RuntimeErrorCode::CoreRejected,
                    format!("duplicate gateway entry prefix: {prefix}"),
                ));
            }
        }
        Ok(())
    }

    #[must_use]
    pub const fn address(&self) -> SocketAddr {
        self.address
    }

    #[must_use]
    pub const fn configuration_revision(&self) -> u64 {
        self.configuration_revision
    }

    #[must_use]
    pub fn entry_prefixes(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| entry.prefix.clone())
            .collect()
    }

    #[must_use]
    pub fn entry_statuses(&self) -> Vec<GatewayEntryStatus> {
        self.entries.iter().map(Self::entry_status).collect()
    }

    #[must_use]
    pub fn status(
        &self,
        configured_address: &str,
        configured_port: u16,
        error: Option<String>,
    ) -> GatewayStatus {
        let entries = self.entry_statuses();
        let blocked_entries = entries
            .iter()
            .filter(|entry| entry.blocked)
            .cloned()
            .collect();
        GatewayStatus {
            configured_address: configured_address.to_owned(),
            configured_port,
            bound_address: Some(self.address.ip().to_string()),
            bound_port: Some(self.address.port()),
            running: true,
            entries,
            blocked_entries,
            duplicate_routes: Vec::new(),
            error,
        }
    }

    #[must_use]
    pub fn contains_prefix(&self, prefix: &str) -> bool {
        self.entries.iter().any(|item| item.prefix == prefix)
    }

    /// Updates the health registry of every mounted publisher that can reference
    /// the upstream id. Unknown ids are harmless and remain isolated per state.
    pub async fn set_health(&self, upstream_id: &str, status: HealthStatus) {
        for entry in &self.entries {
            entry.state.set_health(upstream_id.to_owned(), status).await;
        }
    }

    fn entry_status(entry: &GatewayEntry) -> GatewayEntryStatus {
        let audit_healthy = entry.state.audit_healthy();
        GatewayEntryStatus {
            prefix: entry.prefix.clone(),
            publisher_id: entry.publisher_id.clone(),
            mounted: true,
            blocked: !audit_healthy,
            reason: (!audit_healthy)
                .then(|| "审计存储不可用；入口已拒绝新请求，请检查工作区数据库。".to_owned()),
        }
    }

    pub async fn stop(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for LocalGateway {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LocalGateway;
    use std::net::SocketAddr;

    #[tokio::test]
    async fn accepts_loopback_single_listener() {
        let gateway = LocalGateway::start_with_revision(
            "127.0.0.1:0"
                .parse::<SocketAddr>()
                .expect("loopback address"),
            Vec::new(),
            42,
        )
        .await
        .expect("loopback gateway starts");
        assert!(gateway.address().ip().is_loopback());
        assert!(gateway.entry_prefixes().is_empty());
        assert_eq!(gateway.configuration_revision(), 42);
        gateway.stop().await;
    }

    #[tokio::test]
    async fn rejects_public_listener() {
        assert!(
            LocalGateway::start(
                "0.0.0.0:0".parse::<SocketAddr>().expect("address"),
                Vec::new()
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn rejects_duplicate_prefixes_before_binding() {
        assert!(LocalGateway::validate_entry_prefixes(["/direct/a", "/direct/a"]).is_err());
        assert!(
            LocalGateway::validate_entry_prefixes(["/direct/a", "/canvas/default/main"]).is_ok()
        );
    }
}
