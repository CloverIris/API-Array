use crate::graph::WorkflowGraph;
use crate::runtime::RuntimeConfig;
use crate::secret::SecretRef;
use crate::{CoreError, ErrorCode, ValidationIssue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Workspace packages evolve independently from Provider and graph schemas.
///
/// Provider manifests and runtime graphs deliberately remain on Core schema V1;
/// changing the desktop workspace layout must not invalidate a provider YAML.
/// POC reset schema. Versions 1–3 intentionally have no migration path.
pub const WORKSPACE_SCHEMA_VERSION: u32 = 6;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspacePackage {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub runtime_state: WorkspaceRuntimeState,
    pub graph: WorkflowGraph,
    /// Workspace-scoped API wallet. It only keeps references to provider
    /// instances and never owns a plaintext credential.
    #[serde(default)]
    pub wallet: ApiWallet,
    /// Audited local endpoints are separate deployable objects that reference
    /// wallet assets. A wallet asset never becomes reachable by itself.
    #[serde(default)]
    pub direct_endpoints: BTreeMap<String, DirectEndpoint>,
    /// The sole local listener used by both direct wallet access and Canvas
    /// compositions. It is deliberately loopback-only.
    #[serde(default)]
    pub gateway: DirectGateway,
    /// Projects and canvases are the desktop composition model. `graph` stays
    /// as the preserved legacy graph so V1 imports are lossless.
    #[serde(default)]
    pub projects: WorkspaceProjects,
    #[serde(default)]
    pub ui: Value,
    #[serde(default)]
    pub templates: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ApiWallet {
    #[serde(default)]
    pub assets: BTreeMap<String, ApiAsset>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiAsset {
    pub id: String,
    pub provider_instance_id: String,
    pub provider_id: String,
    pub name: String,
    /// Asset availability is an upstream concern, not a local endpoint state.
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub billing: BillingPolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectEndpoint {
    pub id: String,
    pub name: String,
    pub alias: String,
    pub asset_id: String,
    pub token_ref: SecretRef,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub models: Vec<DirectModelMapping>,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default)]
    pub audit_tags: BTreeMap<String, String>,
    #[serde(default)]
    pub billing_override: Option<BillingPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectModelMapping {
    pub public_model: String,
    pub upstream_model: String,
}

const fn default_timeout_ms() -> u64 { 30_000 }
const fn default_max_retries() -> u32 { 2 }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectGateway {
    #[serde(default = "default_gateway_address")]
    pub listen_address: String,
    #[serde(default = "default_gateway_port")]
    pub port: u16,
}

const fn default_gateway_port() -> u16 { 7480 }

const fn default_true() -> bool { true }

fn default_gateway_address() -> String { "127.0.0.1".to_owned() }

impl Default for DirectGateway {
    fn default() -> Self {
        Self { listen_address: default_gateway_address(), port: default_gateway_port() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BillingPolicy {
    #[serde(default)]
    pub monthly_budget_micros: Option<u64>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub rules: Vec<PricingRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingRule {
    pub model_pattern: String,
    /// Micro-units of the configured currency per one million tokens.
    pub input_per_million_micros: u64,
    #[serde(default)]
    pub cached_input_per_million_micros: Option<u64>,
    pub output_per_million_micros: u64,
    pub source: PricingRuleSource,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PricingRuleSource {
    Builtin,
    User,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProjects {
    #[serde(default)]
    pub projects: BTreeMap<String, Project>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub folders: BTreeMap<String, ProjectFolder>,
    #[serde(default)]
    pub canvases: BTreeMap<String, Canvas>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFolder {
    pub id: String,
    pub name: String,
    #[serde(default, alias = "canvas_ids")]
    pub canvas_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Canvas {
    pub id: String,
    pub name: String,
    #[serde(default, alias = "folder_id")]
    pub folder_id: Option<String>,
    pub graph: WorkflowGraph,
    #[serde(default, alias = "applied_graph")]
    pub applied_graph: Option<WorkflowGraph>,
    #[serde(default = "default_revision", alias = "draft_revision")]
    pub draft_revision: u64,
    #[serde(default, alias = "applied_revision")]
    pub applied_revision: u64,
    #[serde(default, alias = "publisher_id")]
    pub publisher_id: Option<String>,
    /// V1 imports may contain more than one Publisher. Preserve them instead
    /// of silently rewriting a user graph; new canvases always have one.
    #[serde(default, alias = "legacy_multi_output")]
    pub legacy_multi_output: bool,
}

const fn default_revision() -> u64 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkspaceRuntimeState {
    #[serde(default)]
    pub enabled_publishers: BTreeSet<String>,
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
    pub direct_endpoint_ready: BTreeMap<String, bool>,
}

impl WorkspacePackage {
    /// 校验工作区运行语义、图结构以及导出安全边界。
    ///
    /// # Errors
    ///
    /// 工作区字段、Runtime、Graph 或敏感数据扫描失败时返回错误。
    pub fn validate(&self) -> Result<(), CoreError> {
        let mut issues = Vec::new();
        if self.schema_version != WORKSPACE_SCHEMA_VERSION {
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
        if self.gateway.listen_address != "127.0.0.1" && self.gateway.listen_address != "::1" {
            issues.push(ValidationIssue::new("gateway.listen_address", "LOOPBACK_REQUIRED", "统一审计网关只能监听本机回环地址"));
        }
        if self.gateway.port < 1024 {
            issues.push(ValidationIssue::new("gateway.port", "INVALID_PORT", "统一审计网关端口必须大于等于 1024"));
        }
        for publisher_id in &self.runtime_state.enabled_publishers {
            if !self.runtime.publishers.contains_key(publisher_id) {
                issues.push(ValidationIssue::new(
                    format!("runtime_state.enabled_publishers.{publisher_id}"),
                    "PUBLISHER_NOT_FOUND",
                    "启用状态引用了不存在的 Publisher",
                ));
            }
        }
        if let Err(error) = self.runtime.clone().compile() {
            append_issues(&mut issues, "runtime", error);
        }
        if let Err(error) = self.graph.validate() {
            append_issues(&mut issues, "graph", error);
        }
        for (asset_id, asset) in &self.wallet.assets {
            if asset_id != &asset.id || asset.id.trim().is_empty() {
                issues.push(ValidationIssue::new(format!("wallet.assets.{asset_id}.id"), "KEY_MISMATCH", "API wallet asset ID must match its map key"));
            }
            if !self.runtime.providers.contains_key(&asset.provider_instance_id) {
                issues.push(ValidationIssue::new(format!("wallet.assets.{asset_id}.provider_instance_id"), "PROVIDER_NOT_FOUND", "API wallet asset references an unknown Provider instance"));
            }
            if asset.name.trim().is_empty() {
                issues.push(ValidationIssue::new(format!("wallet.assets.{asset_id}.name"), "REQUIRED", "API wallet asset name is required"));
            }
        }
        let mut aliases = BTreeSet::new();
        for (endpoint_id, endpoint) in &self.direct_endpoints {
            let path = format!("direct_endpoints.{endpoint_id}");
            if endpoint_id != &endpoint.id || endpoint.name.trim().is_empty() {
                issues.push(ValidationIssue::new(&path, "ENDPOINT_INVALID", "审计直出端点 ID 和名称必须有效"));
            }
            if endpoint.alias.trim().is_empty() || !endpoint.alias.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_') || !aliases.insert(endpoint.alias.clone()) {
                issues.push(ValidationIssue::new(format!("{path}.alias"), "INVALID_OR_DUPLICATE_ALIAS", "直出别名必须唯一且只能包含 ASCII 字母、数字、连字符或下划线"));
            }
            if !self.wallet.assets.contains_key(&endpoint.asset_id) {
                issues.push(ValidationIssue::new(format!("{path}.asset_id"), "ASSET_NOT_FOUND", "审计直出端点引用了不存在的钱包资产"));
            }
            if endpoint.models.is_empty() || endpoint.models.iter().any(|model| model.public_model.trim().is_empty() || model.upstream_model.trim().is_empty()) {
                issues.push(ValidationIssue::new(format!("{path}.models"), "MODEL_MAPPING_REQUIRED", "审计直出端点至少需要一条有效模型映射"));
            }
            if endpoint.timeout_ms == 0 || endpoint.max_retries > 10 {
                issues.push(ValidationIssue::new(&path, "POLICY_INVALID", "直出超时必须大于零且重试次数不能超过 10"));
            }
        }
        for (project_id, project) in &self.projects.projects {
            if project_id != &project.id || project.name.trim().is_empty() {
                issues.push(ValidationIssue::new(format!("projects.projects.{project_id}"), "PROJECT_INVALID", "Project ID and name must be valid"));
            }
            for (folder_id, folder) in &project.folders {
                if folder_id != &folder.id || folder.name.trim().is_empty() {
                    issues.push(ValidationIssue::new(format!("projects.projects.{project_id}.folders.{folder_id}"), "FOLDER_INVALID", "Folder ID and name must be valid"));
                }
                for canvas_id in &folder.canvas_ids {
                    if !project.canvases.contains_key(canvas_id) {
                        issues.push(ValidationIssue::new(format!("projects.projects.{project_id}.folders.{folder_id}.canvas_ids"), "CANVAS_NOT_FOUND", "Folder references an unknown canvas"));
                    }
                }
            }
            for (canvas_id, canvas) in &project.canvases {
                if canvas_id != &canvas.id || canvas.name.trim().is_empty() {
                    issues.push(ValidationIssue::new(format!("projects.projects.{project_id}.canvases.{canvas_id}"), "CANVAS_INVALID", "Canvas ID and name must be valid"));
                }
                if let Some(folder_id) = &canvas.folder_id && !project.folders.contains_key(folder_id) {
                    issues.push(ValidationIssue::new(format!("projects.projects.{project_id}.canvases.{canvas_id}.folder_id"), "FOLDER_NOT_FOUND", "Canvas references an unknown folder"));
                }
                if let Err(error) = canvas.graph.validate() {
                    append_issues(&mut issues, &format!("projects.projects.{project_id}.canvases.{canvas_id}.graph"), error);
                }
                let publisher_nodes = canvas.graph.nodes.iter().filter(|node| node.kind == crate::graph::NodeKind::Publisher).count();
                let composer_nodes = canvas.graph.nodes.iter().filter(|node| node.kind == crate::graph::NodeKind::Composer).count();
                if !canvas.legacy_multi_output && publisher_nodes != 1 {
                    issues.push(ValidationIssue::new(format!("projects.projects.{project_id}.canvases.{canvas_id}.graph"), "CANVAS_OUTPUT_REQUIRED", "New canvases must contain exactly one total Publisher output"));
                }
                if composer_nodes != 1 {
                    issues.push(ValidationIssue::new(format!("projects.projects.{project_id}.canvases.{canvas_id}.graph"), "CANVAS_COMPOSER_REQUIRED", "Canvas 必须且只能包含一个 Composer"));
                }
                if let Some(publisher_id) = &canvas.publisher_id && !self.runtime.publishers.contains_key(publisher_id) {
                    issues.push(ValidationIssue::new(format!("projects.projects.{project_id}.canvases.{canvas_id}.publisher_id"), "PUBLISHER_NOT_FOUND", "Canvas references an unknown Publisher"));
                }
            }
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
        let direct_refs = self.direct_endpoints.values().map(|endpoint| endpoint.token_ref.as_str());
        provider_refs
            .chain(publisher_refs)
            .chain(direct_refs)
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
        let direct_endpoint_ready = self.direct_endpoints.iter().map(|(id, endpoint)| {
            let asset_ready = self.wallet.assets.get(&endpoint.asset_id)
                .and_then(|asset| self.runtime.providers.get(&asset.provider_instance_id))
                .is_some_and(|provider| asset_and_provider_ready(provider, available));
            (id.clone(), available.contains(endpoint.token_ref.as_str()) && asset_ready)
        }).collect();
        WorkspaceSecretStatus {
            required,
            missing,
            publisher_ready,
            direct_endpoint_ready,
        }
    }
}

fn asset_and_provider_ready(provider: &crate::runtime::ProviderInstance, available: &BTreeSet<String>) -> bool {
    provider.enabled && provider.secret_refs.values().all(|reference| available.contains(reference.as_str()))
}

/// 安全加载 POC reset 工作区。旧 V1/V2/V3 一律拒绝加载，不迁移。
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
    if schema_version != WORKSPACE_SCHEMA_VERSION {
        return Err(CoreError::new(ErrorCode::UnsupportedSchema, format!("此 POC 只接受工作区 Schema {WORKSPACE_SCHEMA_VERSION}；旧工作区必须重建")));
    }
    let workspace: WorkspacePackage = serde_json::from_value(raw)?;
    workspace.validate()?;
    Ok(WorkspaceLoad::Ready {
        workspace: Box::new(workspace),
    })
}

#[allow(dead_code)]
fn migrate_workspace_v1(mut raw: Value) -> Result<WorkspacePackage, CoreError> {
    let graph: WorkflowGraph = serde_json::from_value(raw.get("graph").cloned().ok_or_else(|| CoreError::new(ErrorCode::InvalidInput, "V1 workspace is missing graph"))?)?;
    let publisher_ids = raw.pointer("/runtime/publishers").and_then(Value::as_object).map(|publishers| publishers.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
    let legacy_multi_output = graph.nodes.iter().filter(|node| node.kind == crate::graph::NodeKind::Publisher).count() != 1;
    let canvas = Canvas {
        id: "legacy-main".to_owned(),
        name: "Imported main canvas".to_owned(),
        folder_id: Some("default".to_owned()),
        graph,
        applied_graph: None,
        draft_revision: 1,
        applied_revision: 0,
        publisher_id: publisher_ids.first().cloned(),
        legacy_multi_output,
    };
    let project = Project {
        id: "default".to_owned(),
        name: "Default project".to_owned(),
        folders: BTreeMap::from([("default".to_owned(), ProjectFolder { id: "default".to_owned(), name: "Canvases".to_owned(), canvas_ids: vec![canvas.id.clone()] })]),
        canvases: BTreeMap::from([(canvas.id.clone(), canvas)]),
    };
    let assets = raw.pointer("/runtime/providers").and_then(Value::as_object).map(|providers| providers.iter().map(|(id, value)| {
        let provider_id = value.pointer("/manifest/provider/id").and_then(Value::as_str).unwrap_or("custom-openai-compatible").to_owned();
        let name = value.pointer("/manifest/provider/name").and_then(Value::as_str).unwrap_or(id).to_owned();
        (id.clone(), ApiAsset { id: id.clone(), provider_instance_id: id.clone(), provider_id, name, enabled: false, billing: BillingPolicy::default() })
    }).collect()).unwrap_or_default();
    raw["schema_version"] = Value::from(WORKSPACE_SCHEMA_VERSION);
    raw["wallet"] = serde_json::to_value(ApiWallet { assets })?;
    raw["projects"] = serde_json::to_value(WorkspaceProjects {
        projects: BTreeMap::from([(project.id.clone(), project)]),
    })?;
    serde_json::from_value(raw).map_err(Into::into)
}

#[allow(dead_code)]
fn migrate_workspace_v2(mut raw: Value) -> Result<WorkspacePackage, CoreError> {
    raw["schema_version"] = Value::from(WORKSPACE_SCHEMA_VERSION);
    if let Some(projects) = raw.pointer_mut("/projects") {
        if let Some(object) = projects.as_object_mut() {
            object.remove("activeProjectId");
            object.remove("activeCanvasId");
            object.remove("active_project_id");
            object.remove("active_canvas_id");
        }
        if let Some(project_map) = projects.get_mut("projects").and_then(Value::as_object_mut) {
            for project in project_map.values_mut() {
                if let Some(canvases) = project.get_mut("canvases").and_then(Value::as_object_mut) {
                    for canvas in canvases.values_mut() {
                        if canvas.get("draftRevision").is_none() {
                            canvas["draftRevision"] = Value::from(1);
                        }
                        if canvas.get("appliedRevision").is_none() {
                            canvas["appliedRevision"] = Value::from(0);
                        }
                        if canvas.get("appliedGraph").is_none() {
                            canvas["appliedGraph"] = Value::Null;
                        }
                    }
                }
            }
        }
    }
    serde_json::from_value(raw).map_err(Into::into)
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
    use crate::catalog::builtin_provider_manifests;
    use crate::runtime::{ProviderInstance, RuntimeConfig};

    fn empty_workspace() -> WorkspacePackage {
        WorkspacePackage {
            schema_version: WORKSPACE_SCHEMA_VERSION,
            id: "workspace".to_owned(),
            name: "Workspace".to_owned(),
            runtime: RuntimeConfig {
                schema_version: crate::SCHEMA_VERSION,
                id: "workspace".to_owned(),
                providers: BTreeMap::new(),
                publishers: BTreeMap::new(),
            },
            runtime_state: WorkspaceRuntimeState::default(),
            graph: WorkflowGraph {
                schema_version: crate::graph::GRAPH_SCHEMA_VERSION,
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
            wallet: ApiWallet::default(),
            direct_endpoints: BTreeMap::new(),
            gateway: DirectGateway::default(),
            projects: WorkspaceProjects::default(),
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
    fn non_current_schema_is_rejected_for_poc_reset() -> Result<(), CoreError> {
        let mut value = serde_json::to_value(empty_workspace())?;
        value["schema_version"] = Value::from(WORKSPACE_SCHEMA_VERSION + 1);
        let error = load_workspace_json(&value.to_string()).expect_err("future schema must not load");
        assert_eq!(error.code, ErrorCode::UnsupportedSchema);
        Ok(())
    }

    #[test]
    fn legacy_workspace_is_rejected_instead_of_migrated() -> Result<(), CoreError> {
        let mut value = serde_json::to_value(empty_workspace())?;
        value["schema_version"] = Value::from(1);
        value.as_object_mut().expect("object").remove("wallet");
        value.as_object_mut().expect("object").remove("projects");
        let error = load_workspace_json(&value.to_string()).expect_err("legacy schema must not migrate");
        assert_eq!(error.code, ErrorCode::UnsupportedSchema);
        Ok(())
    }

    #[test]
    fn one_wallet_asset_can_back_multiple_isolated_direct_endpoints() -> Result<(), CoreError> {
        let mut workspace = empty_workspace();
        let manifest = builtin_provider_manifests()?.into_iter().next().expect("builtin provider");
        workspace.runtime.providers.insert("asset-provider".to_owned(), ProviderInstance { id: "asset-provider".to_owned(), manifest: manifest.clone(), endpoint_override: None, secret_refs: BTreeMap::new(), enabled: false });
        workspace.wallet.assets.insert("asset".to_owned(), ApiAsset { id: "asset".to_owned(), provider_instance_id: "asset-provider".to_owned(), provider_id: manifest.provider.id, name: "Asset".to_owned(), enabled: false, billing: BillingPolicy::default() });
        for (id, alias) in [("client-a", "client-a"), ("client-b", "client-b")] {
            workspace.direct_endpoints.insert(id.to_owned(), DirectEndpoint { id: id.to_owned(), name: id.to_owned(), alias: alias.to_owned(), asset_id: "asset".to_owned(), token_ref: SecretRef::parse(format!("secret://direct/{id}/token"))?, enabled: false, models: vec![DirectModelMapping { public_model: "default".to_owned(), upstream_model: "upstream".to_owned() }], timeout_ms: 30_000, max_retries: 2, audit_tags: BTreeMap::new(), billing_override: None });
        }
        workspace.validate()?;
        assert_eq!(workspace.direct_endpoints.len(), 2);
        assert_ne!(workspace.direct_endpoints["client-a"].token_ref, workspace.direct_endpoints["client-b"].token_ref);
        Ok(())
    }
}
