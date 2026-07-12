use crate::{RuntimeError, RuntimeErrorCode};
use crate::resilience::ExecutionTrace;
use apiarray_core::inspection::InspectionReport;
use apiarray_core::workspace::{WorkspaceLoad, WorkspacePackage, load_workspace_json};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DATABASE_FILE: &str = "workspace.sqlite3";
const STORAGE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceRecovery {
    pub loaded: WorkspaceLoad,
    pub recovered_from_backup: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceDescriptor {
    pub id: String,
    pub name: String,
    pub root: PathBuf,
    pub database_path: PathBuf,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceHealth {
    pub healthy: bool,
    pub integrity_message: String,
    pub database_bytes: u64,
    pub revision: u64,
    pub last_backup_at_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceBackup {
    pub path: PathBuf,
    pub created_at_ms: u64,
    pub database_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceLocation { pub id: String, pub name: String, pub root: PathBuf, pub last_opened_at_ms: u64 }

#[derive(Debug, Clone)]
pub struct LauncherRepository { path: PathBuf }

impl LauncherRepository {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self { Self { path: path.into() } }
    fn connect(&self) -> Result<Connection, RuntimeError> {
        if let Some(parent) = self.path.parent() { fs::create_dir_all(parent).map_err(storage_error)?; }
        let connection = Connection::open(&self.path).map_err(database_error)?;
        connection.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS workspaces(id TEXT PRIMARY KEY,name TEXT NOT NULL,root TEXT NOT NULL UNIQUE,last_opened_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS launcher_settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);").map_err(database_error)?;
        Ok(connection)
    }
    pub fn locations(&self) -> Result<Vec<WorkspaceLocation>, RuntimeError> {
        let connection = self.connect()?; let mut statement = connection.prepare("SELECT id,name,root,last_opened_at_ms FROM workspaces ORDER BY last_opened_at_ms DESC").map_err(database_error)?;
        statement.query_map([], |row| Ok(WorkspaceLocation { id: row.get(0)?, name: row.get(1)?, root: PathBuf::from(row.get::<_, String>(2)?), last_opened_at_ms: u64::try_from(row.get::<_, i64>(3)?).unwrap_or(0) })).map_err(database_error)?.map(|row| row.map_err(database_error)).collect()
    }
    pub fn active_root(&self) -> Result<Option<PathBuf>, RuntimeError> {
        self.connect()?.query_row("SELECT value FROM launcher_settings WHERE key='active_root'", [], |row| row.get::<_, String>(0)).optional().map_err(database_error).map(|value| value.map(PathBuf::from))
    }
    pub fn register_and_activate(&self, descriptor: &WorkspaceDescriptor) -> Result<(), RuntimeError> {
        let connection = self.connect()?; let transaction = connection.unchecked_transaction().map_err(database_error)?;
        transaction.execute("INSERT INTO workspaces(id,name,root,last_opened_at_ms) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET name=excluded.name,root=excluded.root,last_opened_at_ms=excluded.last_opened_at_ms", params![descriptor.id, descriptor.name, descriptor.root.display().to_string(), unix_millis_i64()]).map_err(database_error)?;
        transaction.execute("INSERT INTO launcher_settings(key,value) VALUES('active_root',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [descriptor.root.display().to_string()]).map_err(database_error)?;
        transaction.commit().map_err(database_error)
    }
}

/// Stable persistence boundary used by Runtime and desktop hosts. Business code
/// never receives a SQLite connection and cannot depend on table layout.
pub trait StorageService: Send + Sync {
    fn load_workspace(&self) -> Result<WorkspaceRecovery, RuntimeError>;
    fn save_workspace(&self, workspace: &WorkspacePackage) -> Result<u64, RuntimeError>;
    fn descriptor(&self) -> Result<WorkspaceDescriptor, RuntimeError>;
    fn health(&self) -> Result<WorkspaceHealth, RuntimeError>;
    fn backup(&self) -> Result<WorkspaceBackup, RuntimeError>;
    fn compact(&self) -> Result<(), RuntimeError>;
    fn read_setting(&self, key: &str) -> Result<Option<String>, RuntimeError>;
    fn write_setting(&self, key: &str, value: &str) -> Result<(), RuntimeError>;
    fn record_audit(&self, trace: &ExecutionTrace) -> Result<(), RuntimeError>;
    fn read_audit(&self, limit: usize) -> Result<Vec<ExecutionTrace>, RuntimeError>;
    fn save_inspection(&self, report: &InspectionReport) -> Result<(), RuntimeError>;
    fn load_inspection(&self, provider_id: &str) -> Result<Option<InspectionReport>, RuntimeError>;
}

#[derive(Debug, Clone)]
pub struct SqliteStorageService {
    root: PathBuf,
}

impl SqliteStorageService {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path { &self.root }

    #[must_use]
    pub fn database_path(&self) -> PathBuf { self.root.join(DATABASE_FILE) }

    #[must_use]
    pub fn exists(&self) -> bool { self.database_path().is_file() }

    fn connect(&self) -> Result<Connection, RuntimeError> {
        fs::create_dir_all(&self.root).map_err(storage_error)?;
        for directory in ["attachments", "exports", "backups"] {
            fs::create_dir_all(self.root.join(directory)).map_err(storage_error)?;
        }
        let connection = Connection::open(self.database_path()).map_err(database_error)?;
        connection.busy_timeout(std::time::Duration::from_secs(5)).map_err(database_error)?;
        connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;").map_err(database_error)?;
        initialize_schema(&connection)?;
        Ok(connection)
    }

    fn revision(connection: &Connection) -> Result<u64, RuntimeError> {
        connection.query_row("SELECT revision FROM workspace_meta WHERE singleton=1", [], |row| row.get::<_, i64>(0)).optional().map_err(database_error).and_then(|value| u64::try_from(value.unwrap_or(0)).map_err(|_| database_corrupt()))
    }
}

impl StorageService for SqliteStorageService {
    fn load_workspace(&self) -> Result<WorkspaceRecovery, RuntimeError> {
        if !self.exists() { return Err(storage_unavailable()); }
        let connection = self.connect()?;
        let payload: String = connection.query_row("SELECT payload_json FROM workspace_snapshot WHERE singleton=1", [], |row| row.get(0)).optional().map_err(database_error)?.ok_or_else(storage_unavailable)?;
        let loaded = load_workspace_json(&payload).map_err(RuntimeError::from)?;
        Ok(WorkspaceRecovery { loaded, recovered_from_backup: false })
    }

    fn save_workspace(&self, workspace: &WorkspacePackage) -> Result<u64, RuntimeError> {
        workspace.validate().map_err(RuntimeError::from)?;
        let payload = workspace.export_json().map_err(RuntimeError::from)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction().map_err(database_error)?;
        let revision = next_revision(&transaction)?;
        replace_workspace_rows(&transaction, workspace, &payload, revision)?;
        transaction.commit().map_err(database_error)?;
        Ok(revision)
    }

    fn descriptor(&self) -> Result<WorkspaceDescriptor, RuntimeError> {
        let connection = self.connect()?;
        connection.query_row("SELECT workspace_id, name, revision FROM workspace_meta WHERE singleton=1", [], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))).map_err(database_error).and_then(|(id, name, revision)| Ok(WorkspaceDescriptor { id, name, root: self.root.clone(), database_path: self.database_path(), revision: u64::try_from(revision).map_err(|_| database_corrupt())? }))
    }

    fn health(&self) -> Result<WorkspaceHealth, RuntimeError> {
        let connection = self.connect()?;
        let integrity_message: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0)).map_err(database_error)?;
        let revision = Self::revision(&connection)?;
        let last_backup_at_ms = self.read_setting("last_backup_at_ms")?.and_then(|value| value.parse().ok());
        Ok(WorkspaceHealth { healthy: integrity_message == "ok", integrity_message, database_bytes: fs::metadata(self.database_path()).map(|item| item.len()).unwrap_or(0), revision, last_backup_at_ms })
    }

    fn backup(&self) -> Result<WorkspaceBackup, RuntimeError> {
        let connection = self.connect()?;
        connection.execute_batch("PRAGMA wal_checkpoint(FULL);").map_err(database_error)?;
        let created_at_ms = unix_millis();
        let path = self.root.join("backups").join(format!("workspace-{created_at_ms}.sqlite3"));
        let mut destination = Connection::open(&path).map_err(database_error)?;
        let backup = rusqlite::backup::Backup::new(&connection, &mut destination).map_err(database_error)?;
        backup.run_to_completion(16, std::time::Duration::from_millis(5), None).map_err(database_error)?;
        drop(backup);
        destination.close().map_err(|(_, error)| database_error(error))?;
        drop(connection);
        self.write_setting("last_backup_at_ms", &created_at_ms.to_string())?;
        Ok(WorkspaceBackup { database_bytes: fs::metadata(&path).map_err(storage_error)?.len(), path, created_at_ms })
    }

    fn compact(&self) -> Result<(), RuntimeError> {
        self.connect()?.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;").map_err(database_error)
    }

    fn read_setting(&self, key: &str) -> Result<Option<String>, RuntimeError> {
        self.connect()?.query_row("SELECT value FROM settings WHERE key=?1", [key], |row| row.get(0)).optional().map_err(database_error)
    }

    fn write_setting(&self, key: &str, value: &str) -> Result<(), RuntimeError> {
        self.connect()?.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value]).map_err(database_error)?;
        Ok(())
    }

    fn record_audit(&self, trace: &ExecutionTrace) -> Result<(), RuntimeError> {
        let payload = serde_json::to_string(trace).map_err(serialization_error)?;
        let result = serde_json::to_string(&trace.result).map_err(serialization_error)?.trim_matches('"').to_owned();
        self.connect()?.execute("INSERT INTO audit_records(created_at_ms,subject_type,subject_id,model,result,latency_ms,input_tokens,output_tokens,estimated_cost_micros,payload_json) VALUES(?1,'publisher',?2,?3,?4,?5,?6,?7,NULL,?8)", params![i64::try_from(trace.started_at_unix_ms).unwrap_or(i64::MAX), trace.publisher_id, trace.public_model, result, i64::try_from(trace.total_latency_ms).unwrap_or(i64::MAX), trace.input_tokens.and_then(|value| i64::try_from(value).ok()), trace.output_tokens.and_then(|value| i64::try_from(value).ok()), payload]).map_err(database_error)?;
        Ok(())
    }

    fn read_audit(&self, limit: usize) -> Result<Vec<ExecutionTrace>, RuntimeError> {
        let connection = self.connect()?;
        let mut statement = connection.prepare("SELECT payload_json FROM audit_records ORDER BY created_at_ms DESC,id DESC LIMIT ?1").map_err(database_error)?;
        let rows = statement.query_map([i64::try_from(limit.clamp(1, 1_000)).unwrap_or(1_000)], |row| row.get::<_, String>(0)).map_err(database_error)?;
        rows.map(|row| row.map_err(database_error).and_then(|payload| serde_json::from_str(&payload).map_err(serialization_error))).collect()
    }

    fn save_inspection(&self, report: &InspectionReport) -> Result<(), RuntimeError> {
        let payload = serde_json::to_string(report).map_err(serialization_error)?;
        self.connect()?.execute("INSERT INTO inspections(provider_id,updated_at_ms,payload_json) VALUES(?1,?2,?3) ON CONFLICT(provider_id) DO UPDATE SET updated_at_ms=excluded.updated_at_ms,payload_json=excluded.payload_json", params![report.provider_id, unix_millis_i64(), payload]).map_err(database_error)?;
        Ok(())
    }

    fn load_inspection(&self, provider_id: &str) -> Result<Option<InspectionReport>, RuntimeError> {
        self.connect()?.query_row("SELECT payload_json FROM inspections WHERE provider_id=?1", [provider_id], |row| row.get::<_, String>(0)).optional().map_err(database_error)?.map(|payload| serde_json::from_str(&payload).map_err(serialization_error)).transpose()
    }
}

/// Compatibility facade retained while ControlPlane callers migrate to the
/// domain StorageService interface.
#[derive(Debug, Clone)]
pub struct WorkspaceRepository { storage: SqliteStorageService }

impl WorkspaceRepository {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self { Self { storage: SqliteStorageService::new(root) } }
    #[must_use]
    pub fn root(&self) -> &Path { self.storage.root() }
    #[must_use]
    pub fn database_path(&self) -> PathBuf { self.storage.database_path() }
    #[must_use]
    pub fn exists(&self) -> bool { self.storage.exists() }
    pub fn save(&self, workspace: &WorkspacePackage) -> Result<(), RuntimeError> { self.storage.save_workspace(workspace).map(drop) }
    pub fn load(&self) -> Result<WorkspaceRecovery, RuntimeError> { self.storage.load_workspace() }
    pub fn descriptor(&self) -> Result<WorkspaceDescriptor, RuntimeError> { self.storage.descriptor() }
    pub fn health(&self) -> Result<WorkspaceHealth, RuntimeError> { self.storage.health() }
    pub fn backup(&self) -> Result<WorkspaceBackup, RuntimeError> { self.storage.backup() }
    pub fn compact(&self) -> Result<(), RuntimeError> { self.storage.compact() }
    pub fn read_setting(&self, key: &str) -> Result<Option<String>, RuntimeError> { self.storage.read_setting(key) }
    pub fn write_setting(&self, key: &str, value: &str) -> Result<(), RuntimeError> { self.storage.write_setting(key, value) }
    pub fn record_audit(&self, trace: &ExecutionTrace) -> Result<(), RuntimeError> { self.storage.record_audit(trace) }
    pub fn read_audit(&self, limit: usize) -> Result<Vec<ExecutionTrace>, RuntimeError> { self.storage.read_audit(limit) }
    pub fn save_inspection(&self, report: &InspectionReport) -> Result<(), RuntimeError> { self.storage.save_inspection(report) }
    pub fn load_inspection(&self, provider_id: &str) -> Result<Option<InspectionReport>, RuntimeError> { self.storage.load_inspection(provider_id) }
    pub fn clear_workspace_files(&self) -> Result<(), RuntimeError> {
        for suffix in ["", "-wal", "-shm"] { let path = PathBuf::from(format!("{}{}", self.database_path().display(), suffix)); if path.exists() { fs::remove_file(path).map_err(storage_error)?; } }
        Ok(())
    }
}

fn initialize_schema(connection: &Connection) -> Result<(), RuntimeError> {
    connection.execute_batch(&format!(r#"
        CREATE TABLE IF NOT EXISTS storage_schema(version INTEGER NOT NULL);
        INSERT INTO storage_schema(version) SELECT {STORAGE_SCHEMA_VERSION} WHERE NOT EXISTS(SELECT 1 FROM storage_schema);
        CREATE TABLE IF NOT EXISTS workspace_meta(singleton INTEGER PRIMARY KEY CHECK(singleton=1), workspace_id TEXT NOT NULL, name TEXT NOT NULL, schema_version INTEGER NOT NULL, revision INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS workspace_snapshot(singleton INTEGER PRIMARY KEY CHECK(singleton=1), payload_json TEXT NOT NULL, revision INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS provider_instances(id TEXT PRIMARY KEY, payload_json TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS wallet_assets(id TEXT PRIMARY KEY, provider_instance_id TEXT NOT NULL, name TEXT NOT NULL, enabled INTEGER NOT NULL, payload_json TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS direct_endpoints(id TEXT PRIMARY KEY, asset_id TEXT NOT NULL REFERENCES wallet_assets(id) ON DELETE RESTRICT, alias TEXT NOT NULL UNIQUE, enabled INTEGER NOT NULL, payload_json TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS projects(id TEXT PRIMARY KEY, name TEXT NOT NULL, payload_json TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS folders(id TEXT NOT NULL, project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE, name TEXT NOT NULL, payload_json TEXT NOT NULL, PRIMARY KEY(project_id,id));
        CREATE TABLE IF NOT EXISTS canvases(id TEXT NOT NULL, project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE, folder_id TEXT, name TEXT NOT NULL, draft_revision INTEGER NOT NULL, applied_revision INTEGER NOT NULL, payload_json TEXT NOT NULL, PRIMARY KEY(project_id,id));
        CREATE TABLE IF NOT EXISTS publishers(id TEXT PRIMARY KEY, payload_json TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS inspections(provider_id TEXT PRIMARY KEY, updated_at_ms INTEGER NOT NULL, payload_json TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS audit_records(id INTEGER PRIMARY KEY AUTOINCREMENT, created_at_ms INTEGER NOT NULL, subject_type TEXT NOT NULL, subject_id TEXT NOT NULL, model TEXT, result TEXT NOT NULL, latency_ms INTEGER, input_tokens INTEGER, output_tokens INTEGER, estimated_cost_micros INTEGER, payload_json TEXT NOT NULL);
        CREATE INDEX IF NOT EXISTS audit_subject_time ON audit_records(subject_type,subject_id,created_at_ms DESC);
    "#)).map_err(database_error)?;
    let version: i64 = connection.query_row("SELECT version FROM storage_schema LIMIT 1", [], |row| row.get(0)).map_err(database_error)?;
    if version != i64::from(STORAGE_SCHEMA_VERSION) { return Err(RuntimeError::new(RuntimeErrorCode::WorkspaceStorageUnavailable, "工作区数据库版本不受支持")); }
    Ok(())
}

fn next_revision(transaction: &Transaction<'_>) -> Result<u64, RuntimeError> {
    let current: i64 = transaction.query_row("SELECT revision FROM workspace_meta WHERE singleton=1", [], |row| row.get(0)).optional().map_err(database_error)?.unwrap_or(0);
    u64::try_from(current).map_err(|_| database_corrupt()).and_then(|value| value.checked_add(1).ok_or_else(database_corrupt))
}

fn replace_workspace_rows(transaction: &Transaction<'_>, workspace: &WorkspacePackage, payload: &str, revision: u64) -> Result<(), RuntimeError> {
    for table in ["direct_endpoints", "wallet_assets", "provider_instances", "folders", "canvases", "projects", "publishers"] { transaction.execute(&format!("DELETE FROM {table}"), []).map_err(database_error)?; }
    for provider in workspace.runtime.providers.values() { transaction.execute("INSERT INTO provider_instances(id,payload_json) VALUES(?1,?2)", params![provider.id, serde_json::to_string(provider).map_err(serialization_error)?]).map_err(database_error)?; }
    for asset in workspace.wallet.assets.values() { transaction.execute("INSERT INTO wallet_assets(id,provider_instance_id,name,enabled,payload_json) VALUES(?1,?2,?3,?4,?5)", params![asset.id, asset.provider_instance_id, asset.name, asset.enabled, serde_json::to_string(asset).map_err(serialization_error)?]).map_err(database_error)?; }
    for endpoint in workspace.direct_endpoints.values() { transaction.execute("INSERT INTO direct_endpoints(id,asset_id,alias,enabled,payload_json) VALUES(?1,?2,?3,?4,?5)", params![endpoint.id, endpoint.asset_id, endpoint.alias, endpoint.enabled, serde_json::to_string(endpoint).map_err(serialization_error)?]).map_err(database_error)?; }
    for project in workspace.projects.projects.values() {
        transaction.execute("INSERT INTO projects(id,name,payload_json) VALUES(?1,?2,?3)", params![project.id, project.name, serde_json::to_string(project).map_err(serialization_error)?]).map_err(database_error)?;
        for folder in project.folders.values() { transaction.execute("INSERT INTO folders(id,project_id,name,payload_json) VALUES(?1,?2,?3,?4)", params![folder.id, project.id, folder.name, serde_json::to_string(folder).map_err(serialization_error)?]).map_err(database_error)?; }
        for canvas in project.canvases.values() { transaction.execute("INSERT INTO canvases(id,project_id,folder_id,name,draft_revision,applied_revision,payload_json) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![canvas.id, project.id, canvas.folder_id, canvas.name, canvas.draft_revision, canvas.applied_revision, serde_json::to_string(canvas).map_err(serialization_error)?]).map_err(database_error)?; }
    }
    for publisher in workspace.runtime.publishers.values() { transaction.execute("INSERT INTO publishers(id,payload_json) VALUES(?1,?2)", params![publisher.config.id, serde_json::to_string(publisher).map_err(serialization_error)?]).map_err(database_error)?; }
    let revision_i64 = i64::try_from(revision).map_err(|_| database_corrupt())?;
    transaction.execute("INSERT INTO workspace_meta(singleton,workspace_id,name,schema_version,revision,updated_at_ms) VALUES(1,?1,?2,?3,?4,?5) ON CONFLICT(singleton) DO UPDATE SET workspace_id=excluded.workspace_id,name=excluded.name,schema_version=excluded.schema_version,revision=excluded.revision,updated_at_ms=excluded.updated_at_ms", params![workspace.id, workspace.name, workspace.schema_version, revision_i64, unix_millis_i64()]).map_err(database_error)?;
    transaction.execute("INSERT INTO workspace_snapshot(singleton,payload_json,revision) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET payload_json=excluded.payload_json,revision=excluded.revision", params![payload, revision_i64]).map_err(database_error)?;
    Ok(())
}

fn unix_millis() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)) }
fn unix_millis_i64() -> i64 { i64::try_from(unix_millis()).unwrap_or(i64::MAX) }
fn storage_unavailable() -> RuntimeError { RuntimeError::new(RuntimeErrorCode::WorkspaceStorageUnavailable, "本地工作区数据库不存在") }
fn database_corrupt() -> RuntimeError { RuntimeError::new(RuntimeErrorCode::WorkspaceStorageUnavailable, "本地工作区数据库损坏") }
fn storage_error(_: std::io::Error) -> RuntimeError { RuntimeError::new(RuntimeErrorCode::WorkspaceStorageUnavailable, "本地工作区存储不可用") }
fn database_error(_: rusqlite::Error) -> RuntimeError { RuntimeError::new(RuntimeErrorCode::WorkspaceStorageUnavailable, "本地工作区数据库不可用") }
fn serialization_error(_: serde_json::Error) -> RuntimeError { RuntimeError::new(RuntimeErrorCode::WorkspaceStorageUnavailable, "工作区数据无法序列化") }

#[cfg(test)]
mod tests {
    use super::*;
    use apiarray_core::runtime::RuntimeConfig;
    use apiarray_core::workspace::{ApiWallet, WorkspaceProjects, WorkspaceRuntimeState, WORKSPACE_SCHEMA_VERSION};
    use serde_json::Value;
    use std::collections::BTreeMap;

    fn workspace() -> WorkspacePackage { WorkspacePackage { schema_version: WORKSPACE_SCHEMA_VERSION, id: "workspace".to_owned(), name: "Workspace".to_owned(), runtime: RuntimeConfig { schema_version: 1, id: "workspace".to_owned(), providers: BTreeMap::new(), publishers: BTreeMap::new() }, runtime_state: WorkspaceRuntimeState::default(), wallet: ApiWallet::default(), direct_endpoints: BTreeMap::new(), gateway: apiarray_core::workspace::DirectGateway::default(), projects: WorkspaceProjects::default(), ui: Value::Null, templates: BTreeMap::new() } }
    fn repository() -> WorkspaceRepository {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        WorkspaceRepository::new(std::env::temp_dir().join(format!("apiarray-sqlite-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))))
    }

    #[test]
    fn sqlite_workspace_round_trips_and_normalizes() -> Result<(), RuntimeError> {
        let repository = repository(); repository.save(&workspace())?;
        assert!(repository.database_path().is_file());
        let loaded = repository.load()?; assert!(matches!(loaded.loaded, WorkspaceLoad::Ready { .. }));
        assert_eq!(repository.descriptor()?.revision, 1);
        assert!(repository.health()?.healthy);
        let _ = fs::remove_dir_all(repository.root()); Ok(())
    }

    #[test]
    fn backup_is_valid_sqlite_database() -> Result<(), RuntimeError> {
        let repository = repository(); repository.save(&workspace())?;
        let backup = repository.backup()?; assert!(backup.path.is_file()); assert!(backup.database_bytes > 0);
        let connection = Connection::open(backup.path).map_err(database_error)?;
        let result: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0)).map_err(database_error)?;
        assert_eq!(result, "ok"); let _ = fs::remove_dir_all(repository.root()); Ok(())
    }

    #[test]
    fn launcher_tracks_and_reopens_active_workspace() -> Result<(), RuntimeError> {
        let root = std::env::temp_dir().join(format!("apiarray-launcher-test-{}", std::process::id()));
        let launcher = LauncherRepository::new(root.join("launcher.sqlite3"));
        let descriptor = WorkspaceDescriptor { id: "workspace-a".to_owned(), name: "Workspace A".to_owned(), root: root.join("workspace-a"), database_path: root.join("workspace-a/workspace.sqlite3"), revision: 1 };
        launcher.register_and_activate(&descriptor)?;
        assert_eq!(launcher.active_root()?, Some(descriptor.root.clone()));
        assert_eq!(launcher.locations()?.first().map(|item| item.id.as_str()), Some("workspace-a"));
        let _ = fs::remove_dir_all(root); Ok(())
    }
}
