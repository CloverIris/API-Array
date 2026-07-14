use crate::resilience::ExecutionTrace;
use crate::{RuntimeError, RuntimeErrorCode};
use apiarray_core::workspace::{WorkspaceLoad, WorkspacePackage, load_workspace_json};
use apiarray_core::{
    events::AggregatedNotification, inspection::InspectionReport, secret::SecretRef,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DATABASE_FILE: &str = "workspace.sqlite3";
const STORAGE_SCHEMA_VERSION: u32 = 2;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageTransactionResult {
    pub committed_version: u64,
    pub runtime_reloaded: bool,
    pub gateway_rebuilt: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceLocation {
    pub id: String,
    pub name: String,
    pub root: PathBuf,
    pub last_opened_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredNotification {
    pub id: String,
    pub level: String,
    pub object_id: String,
    pub summary: String,
    pub occurrence_count: u32,
    pub first_seen_at_ms: u64,
    pub last_seen_at_ms: u64,
    pub read: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NotificationQuery {
    pub unread_only: bool,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuditQuery {
    pub limit: usize,
    pub offset: usize,
    pub result: Option<AuditResultFilter>,
    pub publisher_id: Option<String>,
    pub model: Option<String>,
    pub from_ms: Option<u64>,
    pub to_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditResultFilter {
    Success,
    Failure,
    ClientDisconnected,
}

#[derive(Debug, Clone)]
pub struct LauncherRepository {
    path: PathBuf,
}

impl LauncherRepository {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    fn connect(&self) -> Result<Connection, RuntimeError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(storage_error)?;
        }
        let connection = Connection::open(&self.path).map_err(database_error)?;
        connection.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS workspaces(id TEXT PRIMARY KEY,name TEXT NOT NULL,root TEXT NOT NULL UNIQUE,last_opened_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS launcher_settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);").map_err(database_error)?;
        Ok(connection)
    }
    pub fn locations(&self) -> Result<Vec<WorkspaceLocation>, RuntimeError> {
        let connection = self.connect()?;
        let mut statement = connection.prepare("SELECT id,name,root,last_opened_at_ms FROM workspaces ORDER BY last_opened_at_ms DESC").map_err(database_error)?;
        statement
            .query_map([], |row| {
                Ok(WorkspaceLocation {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    root: PathBuf::from(row.get::<_, String>(2)?),
                    last_opened_at_ms: u64::try_from(row.get::<_, i64>(3)?).unwrap_or(0),
                })
            })
            .map_err(database_error)?
            .map(|row| row.map_err(database_error))
            .collect()
    }
    pub fn active_root(&self) -> Result<Option<PathBuf>, RuntimeError> {
        self.connect()?
            .query_row(
                "SELECT value FROM launcher_settings WHERE key='active_root'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(database_error)
            .map(|value| value.map(PathBuf::from))
    }
    pub fn register_and_activate(
        &self,
        descriptor: &WorkspaceDescriptor,
    ) -> Result<(), RuntimeError> {
        let connection = self.connect()?;
        let transaction = connection.unchecked_transaction().map_err(database_error)?;
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
    fn save_workspace_if_revision(
        &self,
        workspace: &WorkspacePackage,
        expected_revision: u64,
    ) -> Result<u64, RuntimeError>;
    fn descriptor(&self) -> Result<WorkspaceDescriptor, RuntimeError>;
    fn health(&self) -> Result<WorkspaceHealth, RuntimeError>;
    fn backup(&self) -> Result<WorkspaceBackup, RuntimeError>;
    fn compact(&self) -> Result<(), RuntimeError>;
    fn read_setting(&self, key: &str) -> Result<Option<String>, RuntimeError>;
    fn write_setting(&self, key: &str, value: &str) -> Result<(), RuntimeError>;
    fn record_audit(&self, trace: &ExecutionTrace) -> Result<(), RuntimeError>;
    fn estimated_cost_current_month(&self, subject_id: &str) -> Result<u64, RuntimeError>;
    fn read_audit(&self, query: AuditQuery) -> Result<Vec<ExecutionTrace>, RuntimeError>;
    fn upsert_notification(
        &self,
        notification: &AggregatedNotification,
    ) -> Result<bool, RuntimeError>;
    fn read_notifications(
        &self,
        query: NotificationQuery,
    ) -> Result<Vec<StoredNotification>, RuntimeError>;
    fn mark_notifications_read(&self, ids: &[String]) -> Result<(), RuntimeError>;
    fn clear_read_notifications(&self) -> Result<usize, RuntimeError>;
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
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn database_path(&self) -> PathBuf {
        self.root.join(DATABASE_FILE)
    }

    #[must_use]
    pub fn exists(&self) -> bool {
        self.database_path().is_file()
    }

    fn connect(&self) -> Result<Connection, RuntimeError> {
        fs::create_dir_all(&self.root).map_err(storage_error)?;
        for directory in ["attachments", "exports", "backups"] {
            fs::create_dir_all(self.root.join(directory)).map_err(storage_error)?;
        }
        let connection = Connection::open(self.database_path()).map_err(database_error)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(database_error)?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
            )
            .map_err(database_error)?;
        initialize_schema(&connection)?;
        Ok(connection)
    }

    fn revision(connection: &Connection) -> Result<u64, RuntimeError> {
        connection
            .query_row(
                "SELECT revision FROM workspace_meta WHERE singleton=1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(database_error)
            .and_then(|value| u64::try_from(value.unwrap_or(0)).map_err(|_| database_corrupt()))
    }
}

impl StorageService for SqliteStorageService {
    fn load_workspace(&self) -> Result<WorkspaceRecovery, RuntimeError> {
        if !self.exists() {
            return Err(storage_unavailable());
        }
        let connection = self.connect()?;
        let payload: String = connection
            .query_row(
                "SELECT payload_json FROM workspace_snapshot WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(database_error)?
            .ok_or_else(storage_unavailable)?;
        let loaded = load_workspace_json(&payload).map_err(RuntimeError::from)?;
        Ok(WorkspaceRecovery {
            loaded,
            recovered_from_backup: false,
        })
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

    fn save_workspace_if_revision(
        &self,
        workspace: &WorkspacePackage,
        expected_revision: u64,
    ) -> Result<u64, RuntimeError> {
        workspace.validate().map_err(RuntimeError::from)?;
        let payload = workspace.export_json().map_err(RuntimeError::from)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction().map_err(database_error)?;
        let actual_revision = Self::revision(&transaction)?;
        if actual_revision != expected_revision {
            return Err(RuntimeError::new(
                RuntimeErrorCode::WorkspaceConflict,
                "工作区已被其他操作更新，请刷新后重试",
            ));
        }
        let revision = next_revision(&transaction)?;
        replace_workspace_rows(&transaction, workspace, &payload, revision)?;
        transaction.commit().map_err(database_error)?;
        Ok(revision)
    }

    fn descriptor(&self) -> Result<WorkspaceDescriptor, RuntimeError> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT workspace_id, name, revision FROM workspace_meta WHERE singleton=1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .map_err(database_error)
            .and_then(|(id, name, revision)| {
                Ok(WorkspaceDescriptor {
                    id,
                    name,
                    root: self.root.clone(),
                    database_path: self.database_path(),
                    revision: u64::try_from(revision).map_err(|_| database_corrupt())?,
                })
            })
    }

    fn health(&self) -> Result<WorkspaceHealth, RuntimeError> {
        let connection = self.connect()?;
        let integrity_message: String = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .map_err(database_error)?;
        let revision = Self::revision(&connection)?;
        let last_backup_at_ms = self
            .read_setting("last_backup_at_ms")?
            .and_then(|value| value.parse().ok());
        Ok(WorkspaceHealth {
            healthy: integrity_message == "ok",
            integrity_message,
            database_bytes: fs::metadata(self.database_path())
                .map(|item| item.len())
                .unwrap_or(0),
            revision,
            last_backup_at_ms,
        })
    }

    fn backup(&self) -> Result<WorkspaceBackup, RuntimeError> {
        let connection = self.connect()?;
        connection
            .execute_batch("PRAGMA wal_checkpoint(FULL);")
            .map_err(database_error)?;
        let created_at_ms = unix_millis();
        let path = self
            .root
            .join("backups")
            .join(format!("workspace-{created_at_ms}.sqlite3"));
        let mut destination = Connection::open(&path).map_err(database_error)?;
        let backup =
            rusqlite::backup::Backup::new(&connection, &mut destination).map_err(database_error)?;
        backup
            .run_to_completion(16, std::time::Duration::from_millis(5), None)
            .map_err(database_error)?;
        drop(backup);
        destination
            .close()
            .map_err(|(_, error)| database_error(error))?;
        drop(connection);
        self.write_setting("last_backup_at_ms", &created_at_ms.to_string())?;
        Ok(WorkspaceBackup {
            database_bytes: fs::metadata(&path).map_err(storage_error)?.len(),
            path,
            created_at_ms,
        })
    }

    fn compact(&self) -> Result<(), RuntimeError> {
        self.connect()?
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")
            .map_err(database_error)
    }

    fn read_setting(&self, key: &str) -> Result<Option<String>, RuntimeError> {
        self.connect()?
            .query_row("SELECT value FROM settings WHERE key=?1", [key], |row| {
                row.get(0)
            })
            .optional()
            .map_err(database_error)
    }

    fn write_setting(&self, key: &str, value: &str) -> Result<(), RuntimeError> {
        self.connect()?.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value]).map_err(database_error)?;
        Ok(())
    }

    fn record_audit(&self, trace: &ExecutionTrace) -> Result<(), RuntimeError> {
        let payload = serde_json::to_string(trace).map_err(serialization_error)?;
        let result = serde_json::to_string(&trace.result)
            .map_err(serialization_error)?
            .trim_matches('"')
            .to_owned();
        self.connect()?.execute("INSERT INTO audit_records(created_at_ms,subject_type,subject_id,model,result,latency_ms,input_tokens,output_tokens,estimated_cost_micros,payload_json) VALUES(?1,'publisher',?2,?3,?4,?5,?6,?7,?8,?9)", params![i64::try_from(trace.started_at_unix_ms).unwrap_or(i64::MAX), trace.publisher_id, trace.public_model, result, i64::try_from(trace.total_latency_ms).unwrap_or(i64::MAX), trace.input_tokens.and_then(|value| i64::try_from(value).ok()), trace.output_tokens.and_then(|value| i64::try_from(value).ok()), trace.estimated_cost_micros.and_then(|value| i64::try_from(value).ok()), payload]).map_err(database_error)?;
        Ok(())
    }

    fn estimated_cost_current_month(&self, subject_id: &str) -> Result<u64, RuntimeError> {
        let value = self.connect()?.query_row(
            "SELECT COALESCE(SUM(estimated_cost_micros),0) FROM audit_records WHERE subject_id=?1 AND strftime('%Y-%m',created_at_ms/1000,'unixepoch')=strftime('%Y-%m','now')",
            [subject_id],
            |row| row.get::<_, i64>(0),
        ).map_err(database_error)?;
        u64::try_from(value).map_err(|_| database_corrupt())
    }

    fn read_audit(&self, query: AuditQuery) -> Result<Vec<ExecutionTrace>, RuntimeError> {
        let connection = self.connect()?;
        let mut statement = connection.prepare("SELECT payload_json FROM audit_records WHERE (?1 IS NULL OR result=?1) AND (?2 IS NULL OR subject_id=?2) AND (?3 IS NULL OR model=?3) AND (?4 IS NULL OR created_at_ms>=?4) AND (?5 IS NULL OR created_at_ms<=?5) ORDER BY created_at_ms DESC,id DESC LIMIT ?6 OFFSET ?7").map_err(database_error)?;
        let result = query.result.map(|value| match value {
            AuditResultFilter::Success => "success",
            AuditResultFilter::Failure => "failure",
            AuditResultFilter::ClientDisconnected => "client_disconnected",
        });
        let rows = statement
            .query_map(
                params![
                    result,
                    query.publisher_id,
                    query.model,
                    query.from_ms.and_then(|value| i64::try_from(value).ok()),
                    query.to_ms.and_then(|value| i64::try_from(value).ok()),
                    i64::try_from(query.limit.clamp(1, 200)).unwrap_or(200),
                    i64::try_from(query.offset).unwrap_or(i64::MAX)
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(database_error)?;
        rows.map(|row| {
            row.map_err(database_error)
                .and_then(|payload| serde_json::from_str(&payload).map_err(serialization_error))
        })
        .collect()
    }

    fn upsert_notification(
        &self,
        notification: &AggregatedNotification,
    ) -> Result<bool, RuntimeError> {
        let level = serde_json::to_string(&notification.level)
            .map_err(serialization_error)?
            .trim_matches('"')
            .to_owned();
        let connection = self.connect()?;
        let previous = connection
            .query_row(
                "SELECT occurrence_count,last_seen_at_ms FROM notifications WHERE id=?1",
                [&notification.key],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(database_error)?;
        let changed = previous.is_none_or(|(count, seen)| {
            count != i64::from(notification.event.occurrence_count)
                || seen != i64::try_from(notification.last_seen_unix_ms).unwrap_or(i64::MAX)
        });
        connection.execute("INSERT INTO notifications(id,level,object_id,summary,occurrence_count,first_seen_at_ms,last_seen_at_ms,read) VALUES(?1,?2,?3,?4,?5,?6,?7,0) ON CONFLICT(id) DO UPDATE SET level=excluded.level,object_id=excluded.object_id,summary=excluded.summary,occurrence_count=excluded.occurrence_count,last_seen_at_ms=excluded.last_seen_at_ms,read=CASE WHEN notifications.occurrence_count<>excluded.occurrence_count OR notifications.last_seen_at_ms<>excluded.last_seen_at_ms THEN 0 ELSE notifications.read END", params![notification.key, level, notification.event.object_id, notification.event.summary, i64::from(notification.event.occurrence_count), i64::try_from(notification.first_seen_unix_ms).unwrap_or(i64::MAX), i64::try_from(notification.last_seen_unix_ms).unwrap_or(i64::MAX)]).map_err(database_error)?;
        Ok(changed)
    }

    fn read_notifications(
        &self,
        query: NotificationQuery,
    ) -> Result<Vec<StoredNotification>, RuntimeError> {
        let connection = self.connect()?;
        let mut statement = connection.prepare("SELECT id,level,object_id,summary,occurrence_count,first_seen_at_ms,last_seen_at_ms,read FROM notifications WHERE (?1=0 OR read=0) ORDER BY last_seen_at_ms DESC,id DESC LIMIT ?2").map_err(database_error)?;
        let rows = statement
            .query_map(
                params![
                    if query.unread_only { 1 } else { 0 },
                    i64::try_from(query.limit.clamp(1, 500)).unwrap_or(500)
                ],
                |row| {
                    Ok(StoredNotification {
                        id: row.get(0)?,
                        level: row.get(1)?,
                        object_id: row.get(2)?,
                        summary: row.get(3)?,
                        occurrence_count: u32::try_from(row.get::<_, i64>(4)?).unwrap_or(u32::MAX),
                        first_seen_at_ms: u64::try_from(row.get::<_, i64>(5)?).unwrap_or(0),
                        last_seen_at_ms: u64::try_from(row.get::<_, i64>(6)?).unwrap_or(0),
                        read: row.get::<_, i64>(7)? != 0,
                    })
                },
            )
            .map_err(database_error)?;
        rows.map(|row| row.map_err(database_error)).collect()
    }

    fn mark_notifications_read(&self, ids: &[String]) -> Result<(), RuntimeError> {
        if ids.is_empty() {
            return Ok(());
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction().map_err(database_error)?;
        for id in ids {
            transaction
                .execute("UPDATE notifications SET read=1 WHERE id=?1", [id])
                .map_err(database_error)?;
        }
        transaction.commit().map_err(database_error)
    }

    fn clear_read_notifications(&self) -> Result<usize, RuntimeError> {
        self.connect()?
            .execute("DELETE FROM notifications WHERE read=1", [])
            .map_err(database_error)
    }

    fn save_inspection(&self, report: &InspectionReport) -> Result<(), RuntimeError> {
        let payload = serde_json::to_string(report).map_err(serialization_error)?;
        self.connect()?.execute("INSERT INTO inspections(provider_id,updated_at_ms,payload_json) VALUES(?1,?2,?3) ON CONFLICT(provider_id) DO UPDATE SET updated_at_ms=excluded.updated_at_ms,payload_json=excluded.payload_json", params![report.provider_id, unix_millis_i64(), payload]).map_err(database_error)?;
        Ok(())
    }

    fn load_inspection(&self, provider_id: &str) -> Result<Option<InspectionReport>, RuntimeError> {
        self.connect()?
            .query_row(
                "SELECT payload_json FROM inspections WHERE provider_id=?1",
                [provider_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(database_error)?
            .map(|payload| serde_json::from_str(&payload).map_err(serialization_error))
            .transpose()
    }
}

/// Compatibility facade retained while ControlPlane callers migrate to the
/// domain StorageService interface.
#[derive(Debug, Clone)]
pub struct WorkspaceRepository {
    storage: SqliteStorageService,
}

impl WorkspaceRepository {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            storage: SqliteStorageService::new(root),
        }
    }
    #[must_use]
    pub fn root(&self) -> &Path {
        self.storage.root()
    }
    #[must_use]
    pub fn database_path(&self) -> PathBuf {
        self.storage.database_path()
    }
    #[must_use]
    pub fn exists(&self) -> bool {
        self.storage.exists()
    }
    pub fn save(&self, workspace: &WorkspacePackage) -> Result<(), RuntimeError> {
        self.storage.save_workspace(workspace).map(drop)
    }
    pub fn save_if_revision(
        &self,
        workspace: &WorkspacePackage,
        expected_revision: u64,
    ) -> Result<u64, RuntimeError> {
        self.storage
            .save_workspace_if_revision(workspace, expected_revision)
    }
    pub fn load(&self) -> Result<WorkspaceRecovery, RuntimeError> {
        self.storage.load_workspace()
    }
    /// Reads only secret reference strings from the persisted payload. This is
    /// intentionally tolerant of an unsupported workspace schema and never
    /// returns secret values.
    pub fn referenced_secret_refs(&self) -> Result<Vec<SecretRef>, RuntimeError> {
        let connection = self.storage.connect()?;
        let payload: Option<String> = connection
            .query_row(
                "SELECT payload_json FROM workspace_snapshot WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(database_error)?;
        let Some(payload) = payload else {
            return Ok(Vec::new());
        };
        let value: serde_json::Value =
            serde_json::from_str(&payload).map_err(|_| database_corrupt())?;
        let mut references = Vec::new();
        collect_secret_refs(&value, &mut references);
        references.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        references.dedup_by(|left, right| left.as_str() == right.as_str());
        Ok(references)
    }
    pub fn descriptor(&self) -> Result<WorkspaceDescriptor, RuntimeError> {
        self.storage.descriptor()
    }
    pub fn health(&self) -> Result<WorkspaceHealth, RuntimeError> {
        self.storage.health()
    }
    pub fn backup(&self) -> Result<WorkspaceBackup, RuntimeError> {
        self.storage.backup()
    }
    pub fn compact(&self) -> Result<(), RuntimeError> {
        self.storage.compact()
    }
    pub fn read_setting(&self, key: &str) -> Result<Option<String>, RuntimeError> {
        self.storage.read_setting(key)
    }
    pub fn write_setting(&self, key: &str, value: &str) -> Result<(), RuntimeError> {
        self.storage.write_setting(key, value)
    }
    pub fn record_audit(&self, trace: &ExecutionTrace) -> Result<(), RuntimeError> {
        self.storage.record_audit(trace)
    }
    pub fn estimated_cost_current_month(&self, subject_id: &str) -> Result<u64, RuntimeError> {
        self.storage.estimated_cost_current_month(subject_id)
    }
    pub fn read_audit(&self, limit: usize) -> Result<Vec<ExecutionTrace>, RuntimeError> {
        self.storage.read_audit(AuditQuery {
            limit,
            ..AuditQuery::default()
        })
    }
    pub fn query_audit(&self, query: AuditQuery) -> Result<Vec<ExecutionTrace>, RuntimeError> {
        self.storage.read_audit(query)
    }
    pub fn upsert_notification(
        &self,
        notification: &AggregatedNotification,
    ) -> Result<bool, RuntimeError> {
        self.storage.upsert_notification(notification)
    }
    pub fn read_notifications(
        &self,
        query: NotificationQuery,
    ) -> Result<Vec<StoredNotification>, RuntimeError> {
        self.storage.read_notifications(query)
    }
    pub fn mark_notifications_read(&self, ids: &[String]) -> Result<(), RuntimeError> {
        self.storage.mark_notifications_read(ids)
    }
    pub fn clear_read_notifications(&self) -> Result<usize, RuntimeError> {
        self.storage.clear_read_notifications()
    }
    pub fn save_inspection(&self, report: &InspectionReport) -> Result<(), RuntimeError> {
        self.storage.save_inspection(report)
    }
    pub fn load_inspection(
        &self,
        provider_id: &str,
    ) -> Result<Option<InspectionReport>, RuntimeError> {
        self.storage.load_inspection(provider_id)
    }
    pub fn clear_workspace_files(&self) -> Result<(), RuntimeError> {
        for suffix in ["", "-wal", "-shm"] {
            let path = PathBuf::from(format!("{}{}", self.database_path().display(), suffix));
            if path.exists() {
                fs::remove_file(path).map_err(storage_error)?;
            }
        }
        Ok(())
    }
}

fn collect_secret_refs(value: &serde_json::Value, output: &mut Vec<SecretRef>) {
    match value {
        serde_json::Value::String(text) if text.starts_with("secret://") => {
            if let Ok(reference) = SecretRef::parse(text) {
                output.push(reference);
            }
        }
        serde_json::Value::Array(values) => values
            .iter()
            .for_each(|item| collect_secret_refs(item, output)),
        serde_json::Value::Object(values) => values
            .values()
            .for_each(|item| collect_secret_refs(item, output)),
        _ => {}
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
        CREATE TABLE IF NOT EXISTS notifications(id TEXT PRIMARY KEY,level TEXT NOT NULL,object_id TEXT NOT NULL,summary TEXT NOT NULL,occurrence_count INTEGER NOT NULL,first_seen_at_ms INTEGER NOT NULL,last_seen_at_ms INTEGER NOT NULL,read INTEGER NOT NULL DEFAULT 0);
        CREATE INDEX IF NOT EXISTS notifications_seen ON notifications(read,last_seen_at_ms DESC);
    "#)).map_err(database_error)?;
    let version: i64 = connection
        .query_row("SELECT version FROM storage_schema LIMIT 1", [], |row| {
            row.get(0)
        })
        .map_err(database_error)?;
    if version > i64::from(STORAGE_SCHEMA_VERSION) {
        return Err(RuntimeError::new(
            RuntimeErrorCode::WorkspaceStorageUnavailable,
            "工作区数据库版本高于当前应用支持范围",
        ));
    }
    if version < i64::from(STORAGE_SCHEMA_VERSION) {
        connection
            .execute(
                "UPDATE storage_schema SET version=?1",
                [i64::from(STORAGE_SCHEMA_VERSION)],
            )
            .map_err(database_error)?;
    }
    Ok(())
}

fn next_revision(transaction: &Transaction<'_>) -> Result<u64, RuntimeError> {
    let current: i64 = transaction
        .query_row(
            "SELECT revision FROM workspace_meta WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(database_error)?
        .unwrap_or(0);
    u64::try_from(current)
        .map_err(|_| database_corrupt())
        .and_then(|value| value.checked_add(1).ok_or_else(database_corrupt))
}

fn replace_workspace_rows(
    transaction: &Transaction<'_>,
    workspace: &WorkspacePackage,
    payload: &str,
    revision: u64,
) -> Result<(), RuntimeError> {
    for table in [
        "direct_endpoints",
        "wallet_assets",
        "provider_instances",
        "folders",
        "canvases",
        "projects",
        "publishers",
    ] {
        transaction
            .execute(&format!("DELETE FROM {table}"), [])
            .map_err(database_error)?;
    }
    for provider in workspace.runtime.providers.values() {
        transaction
            .execute(
                "INSERT INTO provider_instances(id,payload_json) VALUES(?1,?2)",
                params![
                    provider.id,
                    serde_json::to_string(provider).map_err(serialization_error)?
                ],
            )
            .map_err(database_error)?;
    }
    for asset in workspace.wallet.assets.values() {
        transaction.execute("INSERT INTO wallet_assets(id,provider_instance_id,name,enabled,payload_json) VALUES(?1,?2,?3,?4,?5)", params![asset.id, asset.provider_instance_id, asset.name, asset.enabled, serde_json::to_string(asset).map_err(serialization_error)?]).map_err(database_error)?;
    }
    for endpoint in workspace.direct_endpoints.values() {
        transaction.execute("INSERT INTO direct_endpoints(id,asset_id,alias,enabled,payload_json) VALUES(?1,?2,?3,?4,?5)", params![endpoint.id, endpoint.asset_id, endpoint.alias, endpoint.enabled, serde_json::to_string(endpoint).map_err(serialization_error)?]).map_err(database_error)?;
    }
    for project in workspace.projects.projects.values() {
        transaction
            .execute(
                "INSERT INTO projects(id,name,payload_json) VALUES(?1,?2,?3)",
                params![
                    project.id,
                    project.name,
                    serde_json::to_string(project).map_err(serialization_error)?
                ],
            )
            .map_err(database_error)?;
        for folder in project.folders.values() {
            transaction
                .execute(
                    "INSERT INTO folders(id,project_id,name,payload_json) VALUES(?1,?2,?3,?4)",
                    params![
                        folder.id,
                        project.id,
                        folder.name,
                        serde_json::to_string(folder).map_err(serialization_error)?
                    ],
                )
                .map_err(database_error)?;
        }
        for canvas in project.canvases.values() {
            transaction.execute("INSERT INTO canvases(id,project_id,folder_id,name,draft_revision,applied_revision,payload_json) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![canvas.id, project.id, canvas.folder_id, canvas.name, canvas.draft_revision, canvas.applied_revision, serde_json::to_string(canvas).map_err(serialization_error)?]).map_err(database_error)?;
        }
    }
    for publisher in workspace.runtime.publishers.values() {
        transaction
            .execute(
                "INSERT INTO publishers(id,payload_json) VALUES(?1,?2)",
                params![
                    publisher.config.id,
                    serde_json::to_string(publisher).map_err(serialization_error)?
                ],
            )
            .map_err(database_error)?;
    }
    let revision_i64 = i64::try_from(revision).map_err(|_| database_corrupt())?;
    transaction.execute("INSERT INTO workspace_meta(singleton,workspace_id,name,schema_version,revision,updated_at_ms) VALUES(1,?1,?2,?3,?4,?5) ON CONFLICT(singleton) DO UPDATE SET workspace_id=excluded.workspace_id,name=excluded.name,schema_version=excluded.schema_version,revision=excluded.revision,updated_at_ms=excluded.updated_at_ms", params![workspace.id, workspace.name, workspace.schema_version, revision_i64, unix_millis_i64()]).map_err(database_error)?;
    transaction.execute("INSERT INTO workspace_snapshot(singleton,payload_json,revision) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET payload_json=excluded.payload_json,revision=excluded.revision", params![payload, revision_i64]).map_err(database_error)?;
    Ok(())
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}
fn unix_millis_i64() -> i64 {
    i64::try_from(unix_millis()).unwrap_or(i64::MAX)
}
fn storage_unavailable() -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::WorkspaceStorageUnavailable,
        "本地工作区数据库不存在",
    )
}
fn database_corrupt() -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::WorkspaceStorageUnavailable,
        "本地工作区数据库损坏",
    )
}
fn storage_error(_: std::io::Error) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::WorkspaceStorageUnavailable,
        "本地工作区存储不可用",
    )
}
fn database_error(_: rusqlite::Error) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::WorkspaceStorageUnavailable,
        "本地工作区数据库不可用",
    )
}
fn serialization_error(_: serde_json::Error) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::WorkspaceStorageUnavailable,
        "工作区数据无法序列化",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use apiarray_core::runtime::RuntimeConfig;
    use apiarray_core::workspace::{
        ApiWallet, WORKSPACE_SCHEMA_VERSION, WorkspaceProjects, WorkspaceRuntimeState,
    };
    use serde_json::Value;
    use std::collections::BTreeMap;

    fn workspace() -> WorkspacePackage {
        WorkspacePackage {
            schema_version: WORKSPACE_SCHEMA_VERSION,
            id: "workspace".to_owned(),
            name: "Workspace".to_owned(),
            runtime: RuntimeConfig {
                schema_version: 1,
                id: "workspace".to_owned(),
                providers: BTreeMap::new(),
                publishers: BTreeMap::new(),
            },
            runtime_state: WorkspaceRuntimeState::default(),
            wallet: ApiWallet::default(),
            direct_endpoints: BTreeMap::new(),
            gateway: apiarray_core::workspace::DirectGateway::default(),
            projects: WorkspaceProjects::default(),
            ui: Value::Null,
            templates: BTreeMap::new(),
        }
    }
    fn repository() -> WorkspaceRepository {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        WorkspaceRepository::new(std::env::temp_dir().join(format!(
            "apiarray-sqlite-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }

    #[test]
    fn sqlite_workspace_round_trips_and_normalizes() -> Result<(), RuntimeError> {
        let repository = repository();
        repository.save(&workspace())?;
        assert!(repository.database_path().is_file());
        let loaded = repository.load()?;
        assert!(matches!(loaded.loaded, WorkspaceLoad::Ready { .. }));
        assert_eq!(repository.descriptor()?.revision, 1);
        assert!(repository.health()?.healthy);
        let _ = fs::remove_dir_all(repository.root());
        Ok(())
    }

    #[test]
    fn optimistic_workspace_save_rejects_a_stale_revision() -> Result<(), RuntimeError> {
        let repository = repository();
        repository.save(&workspace())?;
        let mut first = workspace();
        first.name = "First writer".to_owned();
        assert_eq!(repository.save_if_revision(&first, 1)?, 2);

        let mut stale = workspace();
        stale.name = "Stale writer".to_owned();
        let error = repository
            .save_if_revision(&stale, 1)
            .expect_err("stale revision must be rejected");
        assert_eq!(error.code, RuntimeErrorCode::WorkspaceConflict);
        let WorkspaceLoad::Ready { workspace: loaded } = repository.load()?.loaded;
        assert_eq!(loaded.name, "First writer");
        let _ = fs::remove_dir_all(repository.root());
        Ok(())
    }

    #[test]
    fn backup_is_valid_sqlite_database() -> Result<(), RuntimeError> {
        let repository = repository();
        repository.save(&workspace())?;
        let backup = repository.backup()?;
        assert!(backup.path.is_file());
        assert!(backup.database_bytes > 0);
        let connection = Connection::open(backup.path).map_err(database_error)?;
        let result: String = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .map_err(database_error)?;
        assert_eq!(result, "ok");
        let _ = fs::remove_dir_all(repository.root());
        Ok(())
    }

    #[test]
    fn launcher_tracks_and_reopens_active_workspace() -> Result<(), RuntimeError> {
        let root =
            std::env::temp_dir().join(format!("apiarray-launcher-test-{}", std::process::id()));
        let launcher = LauncherRepository::new(root.join("launcher.sqlite3"));
        let descriptor = WorkspaceDescriptor {
            id: "workspace-a".to_owned(),
            name: "Workspace A".to_owned(),
            root: root.join("workspace-a"),
            database_path: root.join("workspace-a/workspace.sqlite3"),
            revision: 1,
        };
        launcher.register_and_activate(&descriptor)?;
        assert_eq!(launcher.active_root()?, Some(descriptor.root.clone()));
        assert_eq!(
            launcher.locations()?.first().map(|item| item.id.as_str()),
            Some("workspace-a")
        );
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn persists_and_marks_aggregated_notifications() -> Result<(), RuntimeError> {
        use apiarray_core::events::{HealthTransition, NotificationEvent, NotificationLevel};
        let repository = repository();
        repository.save(&workspace())?;
        let mut center = apiarray_core::events::NotificationCenter::default();
        let record = center.ingest(
            NotificationEvent {
                schema_version: 1,
                object_id: "publisher-a".to_owned(),
                transition: HealthTransition::Failed,
                occurrence_count: 1,
                summary: "safe failure".to_owned(),
            },
            None,
            NotificationLevel::ActionRequired,
            42,
        );
        assert!(repository.upsert_notification(&record)?);
        assert_eq!(
            repository
                .read_notifications(NotificationQuery {
                    unread_only: true,
                    limit: 10
                })?
                .len(),
            1
        );
        repository.mark_notifications_read(&[record.key])?;
        assert!(
            repository
                .read_notifications(NotificationQuery {
                    unread_only: true,
                    limit: 10
                })?
                .is_empty()
        );
        assert_eq!(repository.clear_read_notifications()?, 1);
        let _ = fs::remove_dir_all(repository.root());
        Ok(())
    }
}
