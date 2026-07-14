use crate::publisher::{PublisherServer, PublisherState};
use crate::resilience::{AuditSink, NoopAuditSink};
use crate::secret::SecretResolver;
use crate::transport::{HttpExecutor, TransportConfig};
use crate::{RuntimeError, RuntimeErrorCode};
use apiarray_core::runtime::CompiledRuntime;
use apiarray_core::workspace::WorkspaceRuntimeState;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock, oneshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublisherLifecycle {
    Stopped,
    Starting,
    Running,
    Stopping,
    Paused,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PublisherRuntimeStatus {
    pub publisher_id: String,
    pub lifecycle: PublisherLifecycle,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<SocketAddr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at_unix_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SupervisorSnapshot {
    pub publishers: BTreeMap<String, PublisherRuntimeStatus>,
    pub running_count: usize,
}

struct ManagedPublisher {
    status: PublisherRuntimeStatus,
    shutdown: Option<oneshot::Sender<()>>,
    terminal_state: Option<PublisherLifecycle>,
}

#[derive(Clone)]
pub struct PublisherSupervisor {
    runtime: Arc<CompiledRuntime>,
    transport: HttpExecutor,
    secrets: Arc<dyn SecretResolver>,
    audit: Arc<dyn AuditSink>,
    publishers: Arc<RwLock<BTreeMap<String, ManagedPublisher>>>,
    operation_lock: Arc<Mutex<()>>,
}

impl PublisherSupervisor {
    /// 创建独立于 UI 的本地 Publisher 生命周期管理器。
    ///
    /// # Errors
    ///
    /// HTTP 传输初始化失败时返回错误。
    pub fn new(
        runtime: Arc<CompiledRuntime>,
        secrets: Arc<dyn SecretResolver>,
        audit: Option<Arc<dyn AuditSink>>,
    ) -> Result<Self, RuntimeError> {
        let transport = HttpExecutor::new(TransportConfig::default())?;
        let publishers = runtime
            .summary()
            .publishers
            .keys()
            .map(|publisher_id| {
                (
                    publisher_id.clone(),
                    ManagedPublisher {
                        status: PublisherRuntimeStatus {
                            publisher_id: publisher_id.clone(),
                            lifecycle: PublisherLifecycle::Stopped,
                            address: None,
                            started_at_unix_ms: None,
                            last_error: None,
                            generation: 0,
                        },
                        shutdown: None,
                        terminal_state: None,
                    },
                )
            })
            .collect();
        Ok(Self {
            runtime,
            transport,
            secrets,
            audit: audit.unwrap_or_else(|| Arc::new(NoopAuditSink)),
            publishers: Arc::new(RwLock::new(publishers)),
            operation_lock: Arc::new(Mutex::new(())),
        })
    }

    /// 启动一个 Publisher；已运行实例不会被重复绑定。
    ///
    /// # Errors
    ///
    /// Publisher 不存在、已运行、配置无效或端口绑定失败时返回错误。
    pub async fn start(&self, publisher_id: &str) -> Result<PublisherRuntimeStatus, RuntimeError> {
        let _operation = self.operation_lock.lock().await;
        let generation = {
            let mut publishers = self.publishers.write().await;
            let publisher = publishers.get_mut(publisher_id).ok_or_else(|| {
                RuntimeError::new(RuntimeErrorCode::CoreRejected, "要启动的 Publisher 不存在")
            })?;
            if matches!(
                publisher.status.lifecycle,
                PublisherLifecycle::Starting
                    | PublisherLifecycle::Running
                    | PublisherLifecycle::Stopping
            ) {
                return Err(RuntimeError::new(
                    RuntimeErrorCode::PublisherAlreadyRunning,
                    "Publisher 已在运行或正在切换状态",
                ));
            }
            publisher.status.lifecycle = PublisherLifecycle::Starting;
            publisher.status.last_error = None;
            publisher.status.generation = publisher.status.generation.saturating_add(1);
            publisher.terminal_state = None;
            publisher.status.generation
        };

        let state = PublisherState::with_audit(
            Arc::clone(&self.runtime),
            publisher_id,
            self.transport.clone(),
            Arc::clone(&self.secrets),
            Arc::clone(&self.audit),
        );
        let server = match PublisherServer::bind(state).await {
            Ok(server) => server,
            Err(error) => {
                self.set_failed(publisher_id, generation, &error).await;
                return Err(error);
            }
        };
        let address = server.local_address();
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        {
            let mut publishers = self.publishers.write().await;
            let publisher = publishers.get_mut(publisher_id).ok_or_else(|| {
                RuntimeError::new(RuntimeErrorCode::CoreRejected, "Publisher 生命周期状态丢失")
            })?;
            publisher.status.lifecycle = PublisherLifecycle::Running;
            publisher.status.address = Some(address);
            publisher.status.started_at_unix_ms = Some(unix_millis());
            publisher.shutdown = Some(shutdown_sender);
        }
        self.spawn_server(
            publisher_id.to_owned(),
            generation,
            server,
            shutdown_receiver,
        );
        self.status(publisher_id).await
    }

    /// 暂停 Publisher。暂停会关闭监听端口，但保留其可恢复意图。
    ///
    /// # Errors
    ///
    /// Publisher 不存在或未运行时返回错误。
    pub async fn pause(&self, publisher_id: &str) -> Result<PublisherRuntimeStatus, RuntimeError> {
        self.request_stop(publisher_id, PublisherLifecycle::Paused)
            .await
    }

    /// 显式停止 Publisher，并清除其自动恢复意图。
    ///
    /// # Errors
    ///
    /// Publisher 不存在或未运行时返回错误。
    pub async fn stop(&self, publisher_id: &str) -> Result<PublisherRuntimeStatus, RuntimeError> {
        self.request_stop(publisher_id, PublisherLifecycle::Stopped)
            .await
    }

    /// 恢复运行意图中启用的 Publisher。单个失败不阻塞其他 Publisher。
    pub async fn restore_enabled(
        &self,
        runtime_state: &WorkspaceRuntimeState,
    ) -> BTreeMap<String, Result<PublisherRuntimeStatus, RuntimeError>> {
        let mut results = BTreeMap::new();
        for publisher_id in &runtime_state.enabled_publishers {
            results.insert(publisher_id.clone(), self.start(publisher_id).await);
        }
        results
    }

    #[must_use]
    pub async fn snapshot(&self) -> SupervisorSnapshot {
        let publishers = self
            .publishers
            .read()
            .await
            .iter()
            .map(|(id, publisher)| (id.clone(), publisher.status.clone()))
            .collect::<BTreeMap<_, _>>();
        let running_count = publishers
            .values()
            .filter(|status| status.lifecycle == PublisherLifecycle::Running)
            .count();
        SupervisorSnapshot {
            publishers,
            running_count,
        }
    }

    /// 获取单个 Publisher 的可序列化状态。
    ///
    /// # Errors
    ///
    /// Publisher 不存在时返回错误。
    pub async fn status(&self, publisher_id: &str) -> Result<PublisherRuntimeStatus, RuntimeError> {
        self.publishers
            .read()
            .await
            .get(publisher_id)
            .map(|publisher| publisher.status.clone())
            .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::CoreRejected, "Publisher 不存在"))
    }

    #[must_use]
    pub async fn enabled_publishers(&self) -> BTreeSet<String> {
        self.publishers
            .read()
            .await
            .iter()
            .filter(|(_, publisher)| {
                matches!(
                    publisher.status.lifecycle,
                    PublisherLifecycle::Running | PublisherLifecycle::Starting
                )
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Stop all in-process publisher servers without changing persisted run intent.
    pub async fn shutdown_all(&self) {
        let _operation = self.operation_lock.lock().await;
        let senders = {
            let mut publishers = self.publishers.write().await;
            publishers
                .values_mut()
                .filter_map(|publisher| {
                    publisher.status.lifecycle = PublisherLifecycle::Stopping;
                    publisher.terminal_state = Some(PublisherLifecycle::Stopped);
                    publisher.shutdown.take()
                })
                .collect::<Vec<_>>()
        };
        for sender in senders {
            let _ = sender.send(());
        }
    }

    async fn request_stop(
        &self,
        publisher_id: &str,
        terminal_state: PublisherLifecycle,
    ) -> Result<PublisherRuntimeStatus, RuntimeError> {
        let _operation = self.operation_lock.lock().await;
        let sender = {
            let mut publishers = self.publishers.write().await;
            let publisher = publishers.get_mut(publisher_id).ok_or_else(|| {
                RuntimeError::new(RuntimeErrorCode::CoreRejected, "Publisher 不存在")
            })?;
            let sender = publisher.shutdown.take().ok_or_else(|| {
                RuntimeError::new(
                    RuntimeErrorCode::PublisherNotRunning,
                    "Publisher 当前未运行",
                )
            })?;
            publisher.status.lifecycle = PublisherLifecycle::Stopping;
            publisher.terminal_state = Some(terminal_state);
            sender
        };
        let _ = sender.send(());
        self.status(publisher_id).await
    }

    fn spawn_server(
        &self,
        publisher_id: String,
        generation: u64,
        server: PublisherServer,
        shutdown: oneshot::Receiver<()>,
    ) {
        let publishers = Arc::clone(&self.publishers);
        tokio::spawn(async move {
            let result = server
                .serve_with_shutdown(async {
                    let _ = shutdown.await;
                })
                .await;
            let mut publishers = publishers.write().await;
            let Some(publisher) = publishers.get_mut(&publisher_id) else {
                return;
            };
            if publisher.status.generation != generation {
                return;
            }
            publisher.shutdown = None;
            publisher.status.address = None;
            publisher.status.started_at_unix_ms = None;
            match result {
                Ok(()) => {
                    publisher.status.lifecycle = publisher
                        .terminal_state
                        .take()
                        .unwrap_or(PublisherLifecycle::Stopped);
                }
                Err(error) => {
                    publisher.status.lifecycle = PublisherLifecycle::Failed;
                    publisher.status.last_error = Some(error.safe_message);
                }
            }
        });
    }

    async fn set_failed(&self, publisher_id: &str, generation: u64, error: &RuntimeError) {
        if let Some(publisher) = self.publishers.write().await.get_mut(publisher_id)
            && publisher.status.generation == generation
        {
            publisher.status.lifecycle = PublisherLifecycle::Failed;
            publisher.status.last_error = Some(error.safe_message.clone());
        }
    }
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::{MemorySecretStore, StoreSecretResolver};
    use apiarray_core::provider::ProviderManifest;
    use apiarray_core::publisher::PublisherConfig;
    use apiarray_core::routing::RoutePolicy;
    use apiarray_core::runtime::{
        ModelRoute, ProviderInstance, RuntimeConfig, RuntimePublisher, UpstreamRoute,
    };
    use apiarray_core::secret::SecretRef;
    use std::collections::{BTreeMap, HashSet};

    fn unused_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .expect("bind port")
            .local_addr()
            .expect("local address")
            .port()
    }

    fn runtime() -> Arc<CompiledRuntime> {
        let manifest = ProviderManifest::from_yaml(include_str!("../../../providers/openai.yaml"))
            .expect("valid provider");
        Arc::new(
            RuntimeConfig {
                schema_version: 1,
                id: "runtime".to_owned(),
                providers: BTreeMap::from([(
                    "provider".to_owned(),
                    ProviderInstance {
                        id: "provider".to_owned(),
                        manifest,
                        endpoint_override: None,
                        secret_refs: BTreeMap::from([(
                            "api_key".to_owned(),
                            SecretRef::parse("secret://workspace/provider/api-key").expect("ref"),
                        )]),
                        enabled: true,
                    },
                )]),
                publishers: BTreeMap::from([(
                    "publisher".to_owned(),
                    RuntimePublisher {
                        config: PublisherConfig {
                            schema_version: 1,
                            id: "publisher".to_owned(),
                            name: "Publisher".to_owned(),
                            listen_address: "127.0.0.1".parse().expect("address"),
                            port: unused_port(),
                            base_path: "/v1".to_owned(),
                            require_token: true,
                            token_ref: Some(
                                SecretRef::parse("secret://publisher/test").expect("ref"),
                            ),
                        },
                        routes: vec![ModelRoute {
                            public_model: "smart".to_owned(),
                            policy: RoutePolicy {
                                schema_version: 1,
                                id: "route".to_owned(),
                                timeout_ms: 1_000,
                                max_retries: 0,
                                failover_on: HashSet::new(),
                                selection_strategy:
                                    apiarray_core::routing::SelectionStrategy::PriorityFailover,
                                latency_hysteresis_ms: 25,
                            },
                            upstreams: vec![UpstreamRoute {
                                id: "upstream".to_owned(),
                                provider_instance: "provider".to_owned(),
                                upstream_model: "model".to_owned(),
                                priority: 0,
                                weight: 1,
                                enabled: true,
                                conditions: Vec::new(),
                                billing: Default::default(),
                            }],
                        }],
                        middleware: Vec::new(),
                    },
                )]),
            }
            .compile()
            .expect("compile runtime"),
        )
    }

    #[tokio::test]
    async fn starts_pauses_and_resumes_publisher() -> Result<(), RuntimeError> {
        let store = Arc::new(MemorySecretStore::default());
        let supervisor =
            PublisherSupervisor::new(runtime(), Arc::new(StoreSecretResolver::new(store)), None)?;
        assert_eq!(
            supervisor.start("publisher").await?.lifecycle,
            PublisherLifecycle::Running
        );
        assert_eq!(
            supervisor.pause("publisher").await?.lifecycle,
            PublisherLifecycle::Stopping
        );
        for _ in 0..20 {
            if supervisor.status("publisher").await?.lifecycle == PublisherLifecycle::Paused {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(
            supervisor.status("publisher").await?.lifecycle,
            PublisherLifecycle::Paused
        );
        supervisor.start("publisher").await?;
        supervisor.stop("publisher").await?;
        Ok(())
    }
}
