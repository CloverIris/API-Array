use crate::graph::WorkflowGraph;
use crate::runtime::RuntimeConfig;
use crate::secret::SecretRef;
use crate::{CoreError, ErrorCode, SCHEMA_VERSION, ValidationIssue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspacePackage {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub runtime: RuntimeConfig,
    pub graph: WorkflowGraph,
    #[serde(default)]
    pub ui: Value,
    #[serde(default)]
    pub templates: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum WorkspaceLoad {
    Ready {
        workspace: Box<WorkspacePackage>,
    },
    ReadOnly {
        schema_version: u32,
        reason: String,
        raw: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSecretStatus {
    pub required: BTreeSet<String>,
    pub missing: BTreeSet<String>,
    pub publisher_ready: BTreeMap<String, bool>,
}

impl WorkspacePackage {
    /// 校验工作区运行语义、图结构以及导出安全边界。
    ///
    /// # Errors
    ///
    /// 工作区字段、Runtime、Graph 或敏感数据扫描失败时返回错误。
    pub fn validate(&self) -> Result<(), CoreError> {
        let mut issues = Vec::new();
        if self.schema_version != SCHEMA_VERSION {
            issues.push(ValidationIssue::new(
                "schema_version",
                "UNSUPPORTED_SCHEMA",
                "工作区 Schema 版本不受支持",
            ));
        }
        if self.id.trim().is_empty() {
            issues.push(ValidationIssue::new("id", "REQUIRED", "工作区 ID 不能为空"));
        }
        if self.name.trim().is_empty() {
            issues.push(ValidationIssue::new(
                "name",
                "REQUIRED",
                "工作区名称不能为空",
            ));
        }
        if self.runtime.id != self.id {
            issues.push(ValidationIssue::new(
                "runtime.id",
                "WORKSPACE_ID_MISMATCH",
                "Runtime ID 必须与工作区 ID 一致",
            ));
        }
        if let Err(error) = self.runtime.clone().compile() {
            append_issues(&mut issues, "runtime", error);
        }
        if let Err(error) = self.graph.validate() {
            append_issues(&mut issues, "graph", error);
        }
        let value = serde_json::to_value(self)?;
        scan_sensitive_value(&value, "$", None, &mut issues);
        if issues.is_empty() {
            Ok(())
        } else {
            Err(CoreError::validation(
                ErrorCode::InvalidInput,
                "工作区校验失败",
                issues,
            ))
        }
    }

    /// 生成不含明文凭据且字段顺序稳定的工作区 JSON。
    ///
    /// # Errors
    ///
    /// 工作区无效或 JSON 序列化失败时返回错误。
    pub fn export_json(&self) -> Result<String, CoreError> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(Into::into)
    }

    #[must_use]
    pub fn required_secret_refs(&self) -> BTreeSet<String> {
        let provider_refs = self
            .runtime
            .providers
            .values()
            .flat_map(|provider| provider.secret_refs.values())
            .map(SecretRef::as_str);
        let publisher_refs = self
            .runtime
            .publishers
            .values()
            .filter_map(|publisher| publisher.config.token_ref.as_ref())
            .map(SecretRef::as_str);
        provider_refs
            .chain(publisher_refs)
            .map(str::to_owned)
            .collect()
    }

    #[must_use]
    pub fn secret_status(&self, available: &BTreeSet<String>) -> WorkspaceSecretStatus {
        let required = self.required_secret_refs();
        let missing = required.difference(available).cloned().collect();
        let publisher_ready = self
            .runtime
            .publishers
            .iter()
            .map(|(id, publisher)| {
                let token_ready = publisher
                    .config
                    .token_ref
                    .as_ref()
                    .is_none_or(|reference| available.contains(reference.as_str()));
                let upstreams_ready = publisher
                    .routes
                    .iter()
                    .flat_map(|route| &route.upstreams)
                    .all(|upstream| {
                        self.runtime
                            .providers
                            .get(&upstream.provider_instance)
                            .is_some_and(|provider| {
                                provider
                                    .secret_refs
                                    .values()
                                    .all(|reference| available.contains(reference.as_str()))
                            })
                    });
                (id.clone(), token_ready && upstreams_ready)
            })
            .collect();
        WorkspaceSecretStatus {
            required,
            missing,
            publisher_ready,
        }
    }
}

/// 安全加载工作区；未知版本只读打开，绝不直接投入 Runtime。
///
/// # Errors
///
/// JSON 无效、缺少版本或包含疑似明文凭据时返回错误。
pub fn load_workspace_json(input: &str) -> Result<WorkspaceLoad, CoreError> {
    let raw: Value = serde_json::from_str(input)?;
    let mut issues = Vec::new();
    scan_sensitive_value(&raw, "$", None, &mut issues);
    if !issues.is_empty() {
        return Err(CoreError::validation(
            ErrorCode::SecretInvalid,
            "工作区包含疑似明文凭据，已拒绝导入",
            issues,
        ));
    }
    let schema_version = raw
        .get("schema_version")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| CoreError::new(ErrorCode::UnsupportedSchema, "工作区缺少 Schema 版本"))?;
    if schema_version != SCHEMA_VERSION {
        return Ok(WorkspaceLoad::ReadOnly {
            schema_version,
            reason: format!("当前只支持工作区 Schema {SCHEMA_VERSION}，该文件以只读方式打开"),
            raw,
        });
    }
    let workspace: WorkspacePackage = serde_json::from_value(raw)?;
    workspace.validate()?;
    Ok(WorkspaceLoad::Ready {
        workspace: Box::new(workspace),
    })
}

fn scan_sensitive_value(
    value: &Value,
    path: &str,
    field: Option<&str>,
    issues: &mut Vec<ValidationIssue>,
) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                scan_sensitive_value(child, &format!("{path}.{key}"), Some(key), issues);
            }
        }
        Value::Array(array) => {
            for (index, child) in array.iter().enumerate() {
                scan_sensitive_value(child, &format!("{path}[{index}]"), field, issues);
            }
        }
        Value::String(text) => {
            let field = field.unwrap_or_default().to_ascii_lowercase();
            let sensitive_field = field.contains("api_key")
                || field.contains("password")
                || field.contains("private_key")
                || field.contains("client_secret")
                || field == "authorization"
                || field.ends_with("token")
                || field.ends_with("token_ref");
            let looks_like_secret = (text.starts_with("sk-") && text.len() >= 16)
                || (text.starts_with("AIza") && text.len() >= 24)
                || (text.starts_with("Bearer ") && text.len() >= 24)
                || text.to_ascii_lowercase().contains("api_key=")
                || text.to_ascii_lowercase().contains("access_token=");
            if (sensitive_field && !text.starts_with("secret://")) || looks_like_secret {
                issues.push(ValidationIssue::new(
                    path,
                    "PLAINTEXT_SECRET_FORBIDDEN",
                    "工作区只能保存 secret:// 引用，不能保存疑似明文凭据",
                ));
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn append_issues(issues: &mut Vec<ValidationIssue>, prefix: &str, error: CoreError) {
    if error.issues.is_empty() {
        issues.push(ValidationIssue::new(
            prefix,
            format!("{:?}", error.code).to_ascii_uppercase(),
            error.message,
        ));
    } else {
        issues.extend(error.issues.into_iter().map(|issue| {
            ValidationIssue::new(
                format!("{prefix}.{}", issue.path),
                issue.code,
                issue.message,
            )
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Node, NodeKind};
    use crate::runtime::RuntimeConfig;

    fn empty_workspace() -> WorkspacePackage {
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
            graph: WorkflowGraph {
                schema_version: 1,
                id: "graph".to_owned(),
                nodes: vec![Node {
                    id: "group".to_owned(),
                    name: "Group".to_owned(),
                    kind: NodeKind::Group,
                    enabled: true,
                    inputs: Vec::new(),
                    outputs: Vec::new(),
                    config: Value::Null,
                }],
                edges: Vec::new(),
            },
            ui: serde_json::json!({"positions": {"group": [10, 20]}}),
            templates: BTreeMap::new(),
        }
    }

    #[test]
    fn export_round_trip_is_deterministic() -> Result<(), CoreError> {
        let workspace = empty_workspace();
        let first = workspace.export_json()?;
        let WorkspaceLoad::Ready { workspace } = load_workspace_json(&first)? else {
            panic!("current schema must be ready");
        };
        assert_eq!(first, workspace.export_json()?);
        Ok(())
    }

    #[test]
    fn import_rejects_plaintext_secret_anywhere() {
        let mut value = serde_json::to_value(empty_workspace()).expect("serialize");
        value["ui"]["api_key"] = Value::String("plaintext-credential-value".to_owned());
        let error = load_workspace_json(&value.to_string()).expect_err("secret must fail");
        assert_eq!(error.code, ErrorCode::SecretInvalid);
    }

    #[test]
    fn newer_schema_opens_read_only() -> Result<(), CoreError> {
        let mut value = serde_json::to_value(empty_workspace())?;
        value["schema_version"] = Value::from(2);
        assert!(matches!(
            load_workspace_json(&value.to_string())?,
            WorkspaceLoad::ReadOnly {
                schema_version: 2,
                ..
            }
        ));
        Ok(())
    }
}
