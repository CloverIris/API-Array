use crate::{CoreError, ErrorCode, SCHEMA_VERSION, ValidationIssue};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderManifest {
    pub schema_version: u32,
    pub provider: ProviderIdentity,
    pub adapter: AdapterBinding,
    pub endpoint: EndpointDefinition,
    pub authentication: AuthenticationDefinition,
    #[serde(default)]
    pub headers: Vec<StaticHeader>,
    #[serde(default)]
    pub discovery: DiscoveryDefinition,
    #[serde(default)]
    pub probes: Vec<ProbeDefinition>,
    #[serde(default)]
    pub ui: ProviderUi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderIdentity {
    pub id: String,
    pub name: String,
    pub category: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub docs: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterBinding {
    pub id: String,
    pub min_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointDefinition {
    pub default_base_url: String,
    #[serde(default = "default_true")]
    pub editable: bool,
    #[serde(default = "default_https_scheme")]
    pub allowed_schemes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthenticationType {
    HeaderSecret,
    QuerySecret,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthenticationDefinition {
    #[serde(rename = "type")]
    pub kind: AuthenticationType,
    #[serde(default)]
    pub fields: Vec<AuthenticationField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthenticationField {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub secret: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub header: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DiscoveryDefinition {
    #[serde(default)]
    pub models: DiscoveryStrategy,
    #[serde(default)]
    pub balance: DiscoveryStrategy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DiscoveryStrategy {
    #[serde(default)]
    pub strategy: DiscoveryKind,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DiscoveryKind {
    Endpoint,
    Adapter,
    Static,
    #[default]
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeDefinition {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub side_effect: ProbeSideEffect,
    #[serde(default)]
    pub default_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ProbeSideEffect {
    #[default]
    None,
    Billable,
    Mutating,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderUi {
    #[serde(default)]
    pub setup_help: Option<String>,
}

impl ProviderManifest {
    /// 从 YAML 解析并完整校验 Provider 声明。
    ///
    /// # Errors
    ///
    /// YAML 语法无效或声明违反 Provider 安全约束时返回
    /// [`ErrorCode::ProviderInvalid`]。
    pub fn from_yaml(yaml: &str) -> Result<Self, CoreError> {
        let manifest: Self = serde_norway::from_str(yaml).map_err(|error| {
            CoreError::new(
                ErrorCode::ProviderInvalid,
                format!("Provider YAML 无法解析: {error}"),
            )
        })?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// 校验 Provider Schema、端点、鉴权字段和探测安全性。
    ///
    /// # Errors
    ///
    /// 任一字段无效时返回包含全部已发现问题的
    /// [`ErrorCode::ProviderInvalid`]。
    #[allow(clippy::too_many_lines)]
    pub fn validate(&self) -> Result<(), CoreError> {
        let mut issues = Vec::new();

        if self.schema_version != SCHEMA_VERSION {
            issues.push(ValidationIssue::new(
                "schema_version",
                "UNSUPPORTED_SCHEMA",
                format!(
                    "只支持 Schema {SCHEMA_VERSION}，收到 {}",
                    self.schema_version
                ),
            ));
        }
        validate_identifier("provider.id", &self.provider.id, &mut issues);
        validate_identifier("adapter.id", &self.adapter.id, &mut issues);
        if self.provider.name.trim().is_empty() {
            issues.push(ValidationIssue::new(
                "provider.name",
                "REQUIRED",
                "Provider 名称不能为空",
            ));
        }
        if self.adapter.min_version == 0 {
            issues.push(ValidationIssue::new(
                "adapter.min_version",
                "OUT_OF_RANGE",
                "Adapter 最低版本必须大于 0",
            ));
        }

        let scheme = self
            .endpoint
            .default_base_url
            .split_once("://")
            .map(|(value, _)| value.to_ascii_lowercase());
        match scheme {
            Some(scheme) if self.endpoint.allowed_schemes.contains(&scheme) => {}
            Some(scheme) => issues.push(ValidationIssue::new(
                "endpoint.default_base_url",
                "SCHEME_NOT_ALLOWED",
                format!("URL Scheme {scheme} 不在允许列表中"),
            )),
            None => issues.push(ValidationIssue::new(
                "endpoint.default_base_url",
                "INVALID_URL",
                "Base URL 必须包含明确的 URL Scheme",
            )),
        }

        if self
            .endpoint
            .allowed_schemes
            .iter()
            .any(|scheme| scheme != "https" && scheme != "http")
        {
            issues.push(ValidationIssue::new(
                "endpoint.allowed_schemes",
                "UNSUPPORTED_SCHEME",
                "首版只允许 http 或 https",
            ));
        }

        let mut field_ids = HashSet::new();
        for (index, field) in self.authentication.fields.iter().enumerate() {
            let path = format!("authentication.fields[{index}]");
            validate_identifier(&format!("{path}.id"), &field.id, &mut issues);
            if !field_ids.insert(&field.id) {
                issues.push(ValidationIssue::new(
                    format!("{path}.id"),
                    "DUPLICATE",
                    "鉴权字段 ID 必须唯一",
                ));
            }
            if field.secret && !field.required {
                issues.push(ValidationIssue::new(
                    format!("{path}.required"),
                    "UNSAFE_OPTIONAL_SECRET",
                    "首版 Secret 鉴权字段必须明确标记为必填",
                ));
            }
            if matches!(self.authentication.kind, AuthenticationType::HeaderSecret)
                && field.secret
                && field.header.is_none()
            {
                issues.push(ValidationIssue::new(
                    format!("{path}.header"),
                    "REQUIRED",
                    "Header Secret 必须声明 Header 名称",
                ));
            }
            if matches!(self.authentication.kind, AuthenticationType::QuerySecret)
                && field.secret
                && field.query.is_none()
            {
                issues.push(ValidationIssue::new(
                    format!("{path}.query"),
                    "REQUIRED",
                    "Query Secret 必须声明 Query 参数名称",
                ));
            }
        }

        let mut probe_ids = HashSet::new();
        for (index, probe) in self.probes.iter().enumerate() {
            if !probe_ids.insert(&probe.id) {
                issues.push(ValidationIssue::new(
                    format!("probes[{index}].id"),
                    "DUPLICATE",
                    "探测 ID 必须唯一",
                ));
            }
            if probe.default_enabled && probe.side_effect != ProbeSideEffect::None {
                issues.push(ValidationIssue::new(
                    format!("probes[{index}].default_enabled"),
                    "UNSAFE_DEFAULT",
                    "有费用或副作用的探测不能默认启用",
                ));
            }
        }

        if issues.is_empty() {
            Ok(())
        } else {
            Err(CoreError::validation(
                ErrorCode::ProviderInvalid,
                "Provider 配置校验失败",
                issues,
            ))
        }
    }
}

fn validate_identifier(path: &str, value: &str, issues: &mut Vec<ValidationIssue>) {
    if value.is_empty()
        || !value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
                || character == '_'
                || character == '.'
        })
    {
        issues.push(ValidationIssue::new(
            path,
            "INVALID_IDENTIFIER",
            "标识符只能包含小写字母、数字、点、短横线和下划线",
        ));
    }
}

const fn default_true() -> bool {
    true
}

fn default_https_scheme() -> Vec<String> {
    vec!["https".to_owned()]
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
schema_version: 1
provider:
  id: openai-official
  name: OpenAI
  category: ai-model
adapter:
  id: openai-compatible
  min_version: 1
endpoint:
  default_base_url: https://api.openai.com/v1
authentication:
  type: header-secret
  fields:
    - id: api_key
      label: API Key
      secret: true
      required: true
      header: Authorization
      prefix: "Bearer "
probes:
  - id: models
    kind: metadata
    side_effect: none
    default_enabled: true
"#;

    #[test]
    fn parses_valid_provider() -> Result<(), CoreError> {
        let manifest = ProviderManifest::from_yaml(VALID)?;
        assert_eq!(manifest.provider.id, "openai-official");
        Ok(())
    }

    #[test]
    fn rejects_billable_probe_enabled_by_default() {
        let invalid = VALID.replace("side_effect: none", "side_effect: billable");
        let error = ProviderManifest::from_yaml(&invalid).expect_err("must reject unsafe probe");
        assert!(
            error
                .issues
                .iter()
                .any(|issue| issue.code == "UNSAFE_DEFAULT")
        );
    }
}
