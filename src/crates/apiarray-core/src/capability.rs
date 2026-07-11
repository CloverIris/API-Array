use crate::{CoreError, ErrorCode, SCHEMA_VERSION, ValidationIssue};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityManifest {
    pub schema_version: u32,
    pub source_id: String,
    #[serde(default)]
    pub protocols: BTreeSet<String>,
    #[serde(default)]
    pub capabilities: BTreeMap<String, Capability>,
    #[serde(default)]
    pub models: BTreeMap<String, ModelCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    pub support: SupportLevel,
    pub evidence: Evidence,
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    Unsupported,
    Unknown,
    Partial,
    Supported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    Verified,
    ProviderDeclared,
    AdapterInferred,
    UserDeclared,
    Unknown,
    VerificationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCapability {
    pub public_name: String,
    pub health: ModelHealth,
    pub evidence: Evidence,
    #[serde(default)]
    pub upstreams: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelHealth {
    Available,
    Degraded,
    Unavailable,
    Unknown,
}

impl CapabilityManifest {
    /// 校验能力来源、Schema 和模型映射键。
    ///
    /// # Errors
    ///
    /// Schema 不受支持、来源为空或模型键不一致时返回错误。
    pub fn validate(&self) -> Result<(), CoreError> {
        let mut issues = Vec::new();
        if self.schema_version != SCHEMA_VERSION {
            issues.push(ValidationIssue::new(
                "schema_version",
                "UNSUPPORTED_SCHEMA",
                "Capability Manifest Schema 版本不受支持",
            ));
        }
        if self.source_id.trim().is_empty() {
            issues.push(ValidationIssue::new(
                "source_id",
                "REQUIRED",
                "能力来源不能为空",
            ));
        }
        for (key, model) in &self.models {
            if key != &model.public_name {
                issues.push(ValidationIssue::new(
                    format!("models.{key}.public_name"),
                    "KEY_MISMATCH",
                    "模型映射键必须与 public_name 一致",
                ));
            }
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(CoreError::validation(
                ErrorCode::InvalidInput,
                "Capability Manifest 校验失败",
                issues,
            ))
        }
    }

    /// 合并多个路由来源：协议和模型取并集，能力支持等级取最高可用等级，
    /// 同时保留比“未知”更强的证据来源。
    ///
    /// # Errors
    ///
    /// 输入为空或任意 Manifest 校验失败时返回错误。
    pub fn merge(source_id: impl Into<String>, manifests: &[Self]) -> Result<Self, CoreError> {
        if manifests.is_empty() {
            return Err(CoreError::new(
                ErrorCode::InvalidInput,
                "至少需要一个 Capability Manifest",
            ));
        }

        let mut merged = Self {
            schema_version: SCHEMA_VERSION,
            source_id: source_id.into(),
            protocols: BTreeSet::new(),
            capabilities: BTreeMap::new(),
            models: BTreeMap::new(),
        };

        for manifest in manifests {
            manifest.validate()?;
            merged.protocols.extend(manifest.protocols.iter().cloned());
            for (name, capability) in &manifest.capabilities {
                merged
                    .capabilities
                    .entry(name.clone())
                    .and_modify(|current| {
                        if capability.support > current.support {
                            current.support = capability.support;
                            current.evidence = capability.evidence;
                        }
                        current.attributes.extend(capability.attributes.clone());
                    })
                    .or_insert_with(|| capability.clone());
            }
            for (name, model) in &manifest.models {
                merged
                    .models
                    .entry(name.clone())
                    .and_modify(|current| {
                        current.upstreams.extend(model.upstreams.clone());
                        current.health = merge_health(current.health, model.health);
                        if current.evidence == Evidence::Unknown {
                            current.evidence = model.evidence;
                        }
                    })
                    .or_insert_with(|| model.clone());
            }
        }
        merged.validate()?;
        Ok(merged)
    }
}

const fn merge_health(left: ModelHealth, right: ModelHealth) -> ModelHealth {
    use ModelHealth::{Available, Degraded, Unavailable, Unknown};
    match (left, right) {
        (Available, _) | (_, Available) => Available,
        (Degraded, _) | (_, Degraded) => Degraded,
        (Unknown, other) | (other, Unknown) => other,
        (Unavailable, Unavailable) => Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(source: &str, model_health: ModelHealth) -> CapabilityManifest {
        CapabilityManifest {
            schema_version: 1,
            source_id: source.to_owned(),
            protocols: BTreeSet::from(["openai.chat_completions".to_owned()]),
            capabilities: BTreeMap::from([(
                "streaming".to_owned(),
                Capability {
                    support: SupportLevel::Supported,
                    evidence: Evidence::ProviderDeclared,
                    attributes: BTreeMap::new(),
                },
            )]),
            models: BTreeMap::from([(
                "smart".to_owned(),
                ModelCapability {
                    public_name: "smart".to_owned(),
                    health: model_health,
                    evidence: Evidence::ProviderDeclared,
                    upstreams: BTreeSet::from([source.to_owned()]),
                },
            )]),
        }
    }

    #[test]
    fn merges_model_upstreams_and_health() -> Result<(), CoreError> {
        let merged = CapabilityManifest::merge(
            "publisher/main",
            &[
                manifest("provider/a", ModelHealth::Unavailable),
                manifest("provider/b", ModelHealth::Available),
            ],
        )?;
        let model = &merged.models["smart"];
        assert_eq!(model.health, ModelHealth::Available);
        assert_eq!(model.upstreams.len(), 2);
        Ok(())
    }
}
