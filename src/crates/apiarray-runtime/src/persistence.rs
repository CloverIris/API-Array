use crate::{RuntimeError, RuntimeErrorCode};
use apiarray_core::workspace::{WorkspaceLoad, WorkspacePackage, load_workspace_json};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const WORKSPACE_FILE: &str = "workspace.json";
const BACKUP_FILE: &str = "workspace.backup.json";

#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceRecovery {
    pub loaded: WorkspaceLoad,
    pub recovered_from_backup: bool,
}

#[derive(Debug, Clone)]
pub struct WorkspaceRepository {
    root: PathBuf,
}

impl WorkspaceRepository {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 校验、同步写入工作区，并在替换前保留最近有效副本。
    ///
    /// # Errors
    ///
    /// 目录、序列化、同步或替换文件失败时返回安全错误。
    pub fn save(&self, workspace: &WorkspacePackage) -> Result<(), RuntimeError> {
        let json = workspace.export_json().map_err(RuntimeError::from)?;
        fs::create_dir_all(&self.root).map_err(storage_error)?;
        let primary = self.primary_path();
        if primary.exists() {
            copy_and_sync(&primary, &self.backup_path())?;
        }
        atomic_replace(&primary, json.as_bytes())
    }

    /// 加载工作区；主文件损坏时尝试最近有效副本。
    ///
    /// # Errors
    ///
    /// 主文件和备份均不可用或都不安全时返回错误。
    pub fn load(&self) -> Result<WorkspaceRecovery, RuntimeError> {
        match Self::load_path(&self.primary_path()) {
            Ok(loaded) => Ok(WorkspaceRecovery {
                loaded,
                recovered_from_backup: false,
            }),
            Err(primary_error) => Self::load_path(&self.backup_path())
                .map(|loaded| WorkspaceRecovery {
                    loaded,
                    recovered_from_backup: true,
                })
                .map_err(|_| primary_error),
        }
    }

    /// 删除工作区配置文件；Secret 仍由 `SecretStore` 单独管理。
    ///
    /// # Errors
    ///
    /// 文件系统拒绝删除时返回错误。
    pub fn clear_workspace_files(&self) -> Result<(), RuntimeError> {
        for path in [self.primary_path(), self.backup_path()] {
            if path.exists() {
                fs::remove_file(path).map_err(storage_error)?;
            }
        }
        Ok(())
    }

    fn load_path(path: &Path) -> Result<WorkspaceLoad, RuntimeError> {
        let content = fs::read_to_string(path).map_err(storage_error)?;
        load_workspace_json(&content).map_err(RuntimeError::from)
    }

    fn primary_path(&self) -> PathBuf {
        self.root.join(WORKSPACE_FILE)
    }

    fn backup_path(&self) -> PathBuf {
        self.root.join(BACKUP_FILE)
    }
}

fn copy_and_sync(source: &Path, destination: &Path) -> Result<(), RuntimeError> {
    let bytes = fs::read(source).map_err(storage_error)?;
    write_and_sync(destination, &bytes)
}

fn atomic_replace(destination: &Path, contents: &[u8]) -> Result<(), RuntimeError> {
    let temporary =
        destination.with_extension(format!("tmp-{}-{}", std::process::id(), monotonic_nonce()));
    write_and_sync(&temporary, contents)?;
    fs::rename(&temporary, destination).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        storage_error(error)
    })
}

fn write_and_sync(path: &Path, contents: &[u8]) -> Result<(), RuntimeError> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(storage_error)?;
    file.write_all(contents).map_err(storage_error)?;
    file.sync_all().map_err(storage_error)
}

fn monotonic_nonce() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn storage_error(error: std::io::Error) -> RuntimeError {
    drop(error);
    RuntimeError::new(
        RuntimeErrorCode::WorkspaceStorageUnavailable,
        "本地工作区存储不可用",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use apiarray_core::graph::WorkflowGraph;
    use apiarray_core::runtime::RuntimeConfig;
    use apiarray_core::workspace::WorkspaceRuntimeState;
    use serde_json::Value;
    use std::collections::BTreeMap;

    fn workspace() -> WorkspacePackage {
        WorkspacePackage {
            schema_version: 1,
            id: "workspace".to_owned(),
            name: "Workspace".to_owned(),
            runtime: RuntimeConfig {
                schema_version: 1,
                id: "workspace".to_owned(),
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
            ui: Value::Null,
            templates: BTreeMap::new(),
        }
    }

    fn repository() -> WorkspaceRepository {
        WorkspaceRepository::new(
            std::env::temp_dir().join(format!("apiarray-persistence-test-{}", monotonic_nonce())),
        )
    }

    #[test]
    fn saves_and_loads_workspace() -> Result<(), RuntimeError> {
        let repository = repository();
        repository.save(&workspace())?;
        let loaded = repository.load()?;
        assert!(!loaded.recovered_from_backup);
        assert!(matches!(loaded.loaded, WorkspaceLoad::Ready { .. }));
        repository.clear_workspace_files()?;
        let _ = fs::remove_dir(repository.root());
        Ok(())
    }

    #[test]
    fn recovers_from_last_valid_backup() -> Result<(), RuntimeError> {
        let repository = repository();
        repository.save(&workspace())?;
        repository.save(&workspace())?;
        fs::write(repository.primary_path(), "invalid json").map_err(storage_error)?;
        let loaded = repository.load()?;
        assert!(loaded.recovered_from_backup);
        repository.clear_workspace_files()?;
        let _ = fs::remove_dir(repository.root());
        Ok(())
    }
}
