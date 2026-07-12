use crate::persistence::{WorkspaceRecovery, WorkspaceRepository};
use crate::resilience::AuditSink;
use crate::secret::{SecretStore, StoreSecretResolver};
use crate::supervisor::{PublisherRuntimeStatus, PublisherSupervisor, SupervisorSnapshot};
use crate::{RuntimeError, RuntimeErrorCode};
use apiarray_core::events::{
    AggregatedNotification, HealthTransition, NotificationCenter, NotificationEvent,
    NotificationLevel,
};
use apiarray_core::workspace::{WorkspaceLoad, WorkspacePackage, WorkspaceSecretStatus};
use serde::Serialize;
use std::collections::BTreeSet;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlPlaneSnapshot {
    pub workspace_id: String,
    pub workspace_name: String,
    pub recovered_from_backup: bool,
    pub secrets: WorkspaceSecretStatus,
    pub supervisor: SupervisorSnapshot,
    pub notifications: Vec<AggregatedNotification>,
}

/// UI、CLI 和未来 Tauri 宿主共享的本地 Control Plane 会话。
#[derive(Clone)]
pub struct ControlPlane {
    repository: WorkspaceRepository,
    workspace: Arc<Mutex<WorkspacePackage>>,
    secret_store: Arc<dyn SecretStore>,
    supervisor: PublisherSupervisor,
    notifications: Arc<Mutex<NotificationCenter>>,
    recovered_from_backup: bool,
}

impl ControlPlane {
    /// 从本地工作区打开一个可运行会话；未知 Schema 只读工作区不会启动 Runtime。
    ///
    /// # Errors
    ///
    /// 工作区不存在、只读、配置无效或 Runtime 初始化失败时返回错误。
    pub fn open(
        repository: WorkspaceRepository,
        secret_store: Arc<dyn SecretStore>,
        audit: Option<Arc<dyn AuditSink>>,
    ) -> Result<Self, RuntimeError> {
        let WorkspaceRecovery {
            loaded,
            recovered_from_backup,
        } = repository.load()?;
        let WorkspaceLoad::Ready { workspace } = loaded else {
            return Err(RuntimeError::new(
                RuntimeErrorCode::CoreRejected,
                "工作区版本不受支持，只能以只读方式打开",
            ));
        };
        let runtime = Arc::new(workspace.runtime.clone().compile()?);
        let resolver = Arc::new(StoreSecretResolver::new(Arc::clone(&secret_store)));
        let supervisor = PublisherSupervisor::new(runtime, resolver, audit)?;
        Ok(Self {
            repository,
            workspace: Arc::new(Mutex::new(*workspace)),
            secret_store,
            supervisor,
            notifications: Arc::new(Mutex::new(NotificationCenter::default())),
            recovered_from_backup,
        })
    }

    /// 启动工作区中上次保持启用的 Publisher。缺少 Secret 的 Publisher 会被保留为未启动。
    pub async fn restore_enabled(&self) -> Vec<(String, RuntimeError)> {
        let workspace = self.workspace.lock().await.clone();
        let availability = self.secret_status(&workspace);
        let eligible = apiarray_core::workspace::WorkspaceRuntimeState {
            enabled_publishers: workspace
                .runtime_state
                .enabled_publishers
                .iter()
                .filter(|id| availability.publisher_ready.get(*id) == Some(&true))
                .cloned()
                .collect(),
        };
        let mut errors = workspace
            .runtime_state
            .enabled_publishers
            .iter()
            .filter(|id| availability.publisher_ready.get(*id) != Some(&true))
            .map(|id| {
                (
                    id.clone(),
                    RuntimeError::new(
                        RuntimeErrorCode::SecretUnavailable,
                        "Publisher 缺少本机 Secret，未恢复运行",
                    ),
                )
            })
            .collect::<Vec<_>>();
        errors.extend(
            self.supervisor
                .restore_enabled(&eligible)
                .await
                .into_iter()
                .filter_map(|(id, result)| match result {
                    Ok(_) => None,
                    Err(error) => Some((id, error)),
                }),
        );
        errors
    }

    /// 保存 Secret 后不把明文写入工作区配置。
    ///
    /// # Errors
    ///
    /// 系统凭据库不可用时返回错误。
    pub fn store_secret(
        &self,
        reference: &apiarray_core::secret::SecretRef,
        value: crate::secret::SecretValue,
    ) -> Result<(), RuntimeError> {
        self.secret_store.put(reference, value)
    }

    /// 启动并记录自动恢复意图。
    ///
    /// # Errors
    ///
    /// Publisher 缺少 Secret、配置持久化失败或启动失败时返回错误。
    pub async fn start_publisher(
        &self,
        publisher_id: &str,
    ) -> Result<PublisherRuntimeStatus, RuntimeError> {
        self.require_publisher_secrets(publisher_id).await?;
        self.set_enabled_and_save(publisher_id, true).await?;
        if let Err(error) = self.supervisor.start(publisher_id).await {
            let _ = self.set_enabled_and_save(publisher_id, false).await;
            self.record_failure(publisher_id, &error).await;
            return Err(error);
        }
        self.supervisor.status(publisher_id).await
    }

    /// 暂停 Publisher 并移除自动恢复意图。
    ///
    /// # Errors
    ///
    /// Publisher 未运行或工作区无法保存时返回错误。
    pub async fn pause_publisher(
        &self,
        publisher_id: &str,
    ) -> Result<PublisherRuntimeStatus, RuntimeError> {
        self.set_enabled_and_save(publisher_id, false).await?;
        self.supervisor.pause(publisher_id).await
    }

    /// 停止 Publisher 并移除自动恢复意图。
    ///
    /// # Errors
    ///
    /// Publisher 未运行或工作区无法保存时返回错误。
    pub async fn stop_publisher(
        &self,
        publisher_id: &str,
    ) -> Result<PublisherRuntimeStatus, RuntimeError> {
        self.set_enabled_and_save(publisher_id, false).await?;
        self.supervisor.stop(publisher_id).await
    }

    #[must_use]
    pub async fn snapshot(&self) -> ControlPlaneSnapshot {
        let workspace = self.workspace.lock().await.clone();
        ControlPlaneSnapshot {
            workspace_id: workspace.id.clone(),
            workspace_name: workspace.name.clone(),
            recovered_from_backup: self.recovered_from_backup,
            secrets: self.secret_status(&workspace),
            supervisor: self.supervisor.snapshot().await,
            notifications: self.notifications.lock().await.records(),
        }
    }

    fn secret_status(&self, workspace: &WorkspacePackage) -> WorkspaceSecretStatus {
        let available = workspace
            .required_secret_refs()
            .into_iter()
            .filter(|reference| {
                apiarray_core::secret::SecretRef::parse(reference)
                    .is_ok_and(|reference| self.secret_store.contains(&reference))
            })
            .collect::<BTreeSet<_>>();
        workspace.secret_status(&available)
    }

    async fn require_publisher_secrets(&self, publisher_id: &str) -> Result<(), RuntimeError> {
        let workspace = self.workspace.lock().await.clone();
        let status = self.secret_status(&workspace);
        match status.publisher_ready.get(publisher_id) {
            Some(true) => Ok(()),
            Some(false) => Err(RuntimeError::new(
                RuntimeErrorCode::SecretUnavailable,
                "Publisher 缺少本机 Secret，不能启动",
            )),
            None => Err(RuntimeError::new(
                RuntimeErrorCode::CoreRejected,
                "Publisher 不存在",
            )),
        }
    }

    async fn set_enabled_and_save(
        &self,
        publisher_id: &str,
        enabled: bool,
    ) -> Result<(), RuntimeError> {
        let mut workspace = self.workspace.lock().await;
        if !workspace.runtime.publishers.contains_key(publisher_id) {
            return Err(RuntimeError::new(
                RuntimeErrorCode::CoreRejected,
                "Publisher 不存在",
            ));
        }
        if enabled {
            workspace
                .runtime_state
                .enabled_publishers
                .insert(publisher_id.to_owned());
        } else {
            workspace
                .runtime_state
                .enabled_publishers
                .remove(publisher_id);
        }
        self.repository.save(&workspace)
    }

    async fn record_failure(&self, publisher_id: &str, error: &RuntimeError) {
        let level = if error.code == RuntimeErrorCode::SecretUnavailable {
            NotificationLevel::ActionRequired
        } else {
            NotificationLevel::NotificationCenter
        };
        let _ = self.notifications.lock().await.ingest(
            NotificationEvent {
                schema_version: 1,
                object_id: publisher_id.to_owned(),
                transition: HealthTransition::Failed,
                occurrence_count: 1,
                summary: error.safe_message.clone(),
            },
            error.upstream_error,
            level,
            unix_millis(),
        );
    }
}

fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::MemorySecretStore;
    use apiarray_core::graph::WorkflowGraph;
    use apiarray_core::runtime::RuntimeConfig;
    use apiarray_core::workspace::{ApiWallet, WorkspaceProjects, WorkspaceRuntimeState, WORKSPACE_SCHEMA_VERSION};
    use serde_json::Value;
    use std::collections::BTreeMap;

    fn repository() -> WorkspaceRepository {
        WorkspaceRepository::new(
            std::env::temp_dir().join(format!("apiarray-control-test-{}", unix_millis())),
        )
    }

    fn workspace() -> WorkspacePackage {
        WorkspacePackage {
            schema_version: WORKSPACE_SCHEMA_VERSION,
            id: "control-workspace".to_owned(),
            name: "Control Workspace".to_owned(),
            runtime: RuntimeConfig {
                schema_version: 1,
                id: "control-workspace".to_owned(),
                providers: BTreeMap::new(),
                publishers: BTreeMap::new(),
            },
            runtime_state: WorkspaceRuntimeState::default(),
            graph: WorkflowGraph {
                schema_version: 1,
                id: "graph".to_owned(),
                nodes: Vec::new(),
                edges: Vec::new(),
            },
            wallet: ApiWallet::default(),
            direct_endpoints: BTreeMap::new(),
            gateway: apiarray_core::workspace::DirectGateway::default(),
            projects: WorkspaceProjects::default(),
            ui: Value::Null,
            templates: BTreeMap::new(),
        }
    }

    #[tokio::test]
    async fn opens_workspace_and_reports_host_ready_snapshot() -> Result<(), RuntimeError> {
        let repository = repository();
        repository.save(&workspace())?;
        let control = ControlPlane::open(
            repository.clone(),
            Arc::new(MemorySecretStore::default()),
            None,
        )?;
        let snapshot = control.snapshot().await;
        assert_eq!(snapshot.workspace_id, "control-workspace");
        assert_eq!(snapshot.supervisor.running_count, 0);
        repository.clear_workspace_files()?;
        let _ = std::fs::remove_dir(repository.root());
        Ok(())
    }
}
