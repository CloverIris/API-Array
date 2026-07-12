//! One loopback listener that mounts independent, authenticated PublisherState
//! routes below stable wallet and Canvas prefixes.

use crate::{RuntimeError, RuntimeErrorCode, publisher::PublisherState};
use axum::Router;
use std::net::SocketAddr;
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

pub struct GatewayEntry {
    pub prefix: String,
    pub state: PublisherState,
}

pub struct LocalGateway {
    address: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl LocalGateway {
    pub async fn start(address: SocketAddr, entries: Vec<GatewayEntry>) -> Result<Self, RuntimeError> {
        if !address.ip().is_loopback() {
            return Err(RuntimeError::new(RuntimeErrorCode::PublisherBindFailed, "统一审计网关只能绑定本机回环地址"));
        }
        let listener = TcpListener::bind(address).await.map_err(|_| RuntimeError::new(RuntimeErrorCode::PublisherBindFailed, format!("无法绑定统一审计网关 {address}")))?;
        let address = listener.local_addr().map_err(|_| RuntimeError::new(RuntimeErrorCode::PublisherBindFailed, "无法读取统一审计网关地址"))?;
        let mut router = Router::new();
        for entry in entries {
            if !entry.prefix.starts_with('/') || entry.prefix.contains("..") {
                return Err(RuntimeError::new(RuntimeErrorCode::CoreRejected, "统一审计网关入口前缀无效"));
            }
            router = router.nest(&entry.prefix, entry.state.router());
        }
        let (shutdown, receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router).with_graceful_shutdown(async { let _ = receiver.await; }).await;
        });
        Ok(Self { address, shutdown: Some(shutdown), task: Some(task) })
    }

    #[must_use]
    pub const fn address(&self) -> SocketAddr { self.address }

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
        gateway.stop().await;
    }

    #[tokio::test]
    async fn rejects_public_listener() {
        assert!(LocalGateway::start("0.0.0.0:0".parse::<SocketAddr>().expect("address"), Vec::new()).await.is_err());
    }
}
