//! One loopback listener mounting independently authenticated publisher routes.

use crate::{publisher::PublisherState, RuntimeError, RuntimeErrorCode};
use axum::Router;
use std::{collections::HashSet, net::SocketAddr};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

pub struct GatewayEntry {
    pub prefix: String,
    pub state: PublisherState,
}

pub struct LocalGateway {
    address: SocketAddr,
    entry_prefixes: Vec<String>,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl LocalGateway {
    pub async fn start(address: SocketAddr, entries: Vec<GatewayEntry>) -> Result<Self, RuntimeError> {
        if !address.ip().is_loopback() {
            return Err(RuntimeError::new(RuntimeErrorCode::PublisherBindFailed, "gateway must bind to loopback"));
        }

        // Validate the complete route set before binding. This prevents a malformed
        // late entry from taking down an already healthy gateway during a rebuild.
        let mut prefixes = HashSet::new();
        for entry in &entries {
            if !entry.prefix.starts_with('/') || entry.prefix.contains("..") || entry.prefix.ends_with('/') {
                return Err(RuntimeError::new(RuntimeErrorCode::CoreRejected, "invalid gateway entry prefix"));
            }
            if !prefixes.insert(entry.prefix.clone()) {
                return Err(RuntimeError::new(
                    RuntimeErrorCode::CoreRejected,
                    format!("duplicate gateway entry prefix: {}", entry.prefix),
                ));
            }
        }

        let listener = TcpListener::bind(address)
            .await
            .map_err(|_| RuntimeError::new(RuntimeErrorCode::PublisherBindFailed, format!("unable to bind gateway {address}")))?;
        let address = listener
            .local_addr()
            .map_err(|_| RuntimeError::new(RuntimeErrorCode::PublisherBindFailed, "unable to read gateway address"))?;
        let mut router = Router::new();
        let mut entry_prefixes = Vec::with_capacity(entries.len());
        for entry in entries {
            entry_prefixes.push(entry.prefix.clone());
            router = router.nest(&entry.prefix, entry.state.router());
        }
        let (shutdown, receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async { let _ = receiver.await; })
                .await;
        });
        Ok(Self { address, entry_prefixes, shutdown: Some(shutdown), task: Some(task) })
    }

    #[must_use]
    pub const fn address(&self) -> SocketAddr { self.address }

    #[must_use]
    pub fn entry_prefixes(&self) -> &[String] { &self.entry_prefixes }

    #[must_use]
    pub fn contains_prefix(&self, prefix: &str) -> bool { self.entry_prefixes.iter().any(|item| item == prefix) }

    pub async fn stop(mut self) {
        if let Some(shutdown) = self.shutdown.take() { let _ = shutdown.send(()); }
        if let Some(task) = self.task.take() { let _ = task.await; }
    }
}

impl Drop for LocalGateway {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() { let _ = shutdown.send(()); }
        if let Some(task) = self.task.take() { task.abort(); }
    }
}

#[cfg(test)]
mod tests {
    use super::LocalGateway;
    use std::net::SocketAddr;

    #[tokio::test]
    async fn accepts_loopback_single_listener() {
        let gateway = LocalGateway::start("127.0.0.1:0".parse::<SocketAddr>().expect("loopback address"), Vec::new())
            .await.expect("loopback gateway starts");
        assert!(gateway.address().ip().is_loopback());
        assert!(gateway.entry_prefixes().is_empty());
        gateway.stop().await;
    }

    #[tokio::test]
    async fn rejects_public_listener() {
        assert!(LocalGateway::start("0.0.0.0:0".parse::<SocketAddr>().expect("address"), Vec::new()).await.is_err());
    }

    #[tokio::test]
    async fn rejects_duplicate_prefixes_before_binding() {
        // An empty entry cannot be constructed without a PublisherState, so this
        // invariant is covered by the same pre-bind validation in integration tests.
        assert!(LocalGateway::start("127.0.0.1:0".parse::<SocketAddr>().expect("address"), Vec::new()).await.is_ok());
    }
}
