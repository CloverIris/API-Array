use crate::capability::Evidence;
use crate::provider::{ProbeDefinition, ProbeSideEffect};
use crate::{CoreError, ErrorCode, SCHEMA_VERSION, ValidationIssue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionDimension {
    Authentication,
    Dns,
    Tls,
    Network,
    Models,
    ProtocolCompatibility,
    Streaming,
    ToolCalling,
    VisionInput,
    StructuredOutput,
    ContextWindow,
    Latency,
    RateLimit,
    BalanceQuota,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionStatus {
    Passed,
    Partial,
    Failed,
    Unsupported,
    Unknown,
    NotRun,
    AuthorizationRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectionFinding {
    pub dimension: InspectionDimension,
    pub status: InspectionStatus,
    pub evidence: Evidence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionOverall {
    Healthy,
    Degraded,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectionReport {
    pub schema_version: u32,
    pub provider_id: String,
    pub generated_at_unix_ms: u64,
    pub overall: InspectionOverall,
    pub findings: BTreeMap<InspectionDimension, InspectionFinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success_at_unix_ms: Option<u64>,
    #[serde(default)]
    pub consecutive_failures: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProbeAuthorization {
    #[serde(default)]
    pub allow_billable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeSchedule {
    #[serde(default)]
    pub paused: bool,
    pub minimum_interval_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run_at_unix_ms: Option<u64>,
}

impl ProbeSchedule {
    /// 判断探测是否允许在指定时间运行。
    ///
    /// # Errors
    ///
    /// 探测被暂停、间隔为零或尚未达到频率限制时返回稳定错误。
    pub fn allow_at(&self, now_unix_ms: u64) -> Result<(), CoreError> {
        if self.paused {
            return Err(CoreError::new(ErrorCode::InvalidInput, "探测已暂停"));
        }
        if self.minimum_interval_ms == 0 {
            return Err(CoreError::new(
                ErrorCode::InvalidInput,
                "探测最小间隔必须大于零",
            ));
        }
        if self
            .last_run_at_unix_ms
            .is_some_and(|last| now_unix_ms.saturating_sub(last) < self.minimum_interval_ms)
        {
            return Err(CoreError::new(ErrorCode::InvalidInput, "探测触发过于频繁"));
        }
        Ok(())
    }
}

/// 校验某项 Provider 探测是否可在当前授权下执行。
///
/// # Errors
///
/// 可计费探测未授权或探测具有变更副作用时返回错误。
pub fn authorize_probe(
    probe: &ProbeDefinition,
    authorization: ProbeAuthorization,
) -> Result<(), CoreError> {
    match probe.side_effect {
        ProbeSideEffect::None => Ok(()),
        ProbeSideEffect::Billable if authorization.allow_billable => Ok(()),
        ProbeSideEffect::Billable => Err(CoreError::new(
            ErrorCode::InvalidInput,
            "可计费探测需要用户明确授权",
        )),
        ProbeSideEffect::Mutating => Err(CoreError::new(
            ErrorCode::InvalidInput,
            "首版禁止执行具有资源变更副作用的探测",
        )),
    }
}

impl InspectionReport {
    /// 从探测发现构建体检报告，并计算整体健康度。
    ///
    /// # Errors
    ///
    /// Provider ID、Schema、重复维度或发现内容不合法时返回错误。
    pub fn build(
        provider_id: impl Into<String>,
        generated_at_unix_ms: u64,
        findings: Vec<InspectionFinding>,
        last_success_at_unix_ms: Option<u64>,
        consecutive_failures: u32,
    ) -> Result<Self, CoreError> {
        let provider_id = provider_id.into();
        let mut issues = Vec::new();
        if provider_id.trim().is_empty() {
            issues.push(ValidationIssue::new(
                "provider_id",
                "REQUIRED",
                "Provider ID 不能为空",
            ));
        }
        let mut indexed = BTreeMap::new();
        for (index, finding) in findings.into_iter().enumerate() {
            if finding
                .safe_summary
                .as_ref()
                .is_some_and(|summary| summary.len() > 512)
            {
                issues.push(ValidationIssue::new(
                    format!("findings[{index}].safe_summary"),
                    "TOO_LONG",
                    "体检摘要不能超过 512 个字符",
                ));
            }
            if indexed.insert(finding.dimension, finding).is_some() {
                issues.push(ValidationIssue::new(
                    format!("findings[{index}].dimension"),
                    "DUPLICATE",
                    "同一体检维度只能出现一次",
                ));
            }
        }
        if !issues.is_empty() {
            return Err(CoreError::validation(
                ErrorCode::InvalidInput,
                "体检报告校验失败",
                issues,
            ));
        }
        let overall = overall_status(&indexed);
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            provider_id,
            generated_at_unix_ms,
            overall,
            findings: indexed,
            last_success_at_unix_ms,
            consecutive_failures,
        })
    }
}

fn overall_status(
    findings: &BTreeMap<InspectionDimension, InspectionFinding>,
) -> InspectionOverall {
    let authentication_failed = findings
        .get(&InspectionDimension::Authentication)
        .is_some_and(|finding| finding.status == InspectionStatus::Failed);
    let connectivity_failed = [
        InspectionDimension::Dns,
        InspectionDimension::Tls,
        InspectionDimension::Network,
    ]
    .into_iter()
    .any(|dimension| {
        findings
            .get(&dimension)
            .is_some_and(|finding| finding.status == InspectionStatus::Failed)
    });
    if authentication_failed || connectivity_failed {
        return InspectionOverall::Unavailable;
    }
    if findings.values().any(|finding| {
        matches!(
            finding.status,
            InspectionStatus::Failed | InspectionStatus::Partial
        )
    }) {
        InspectionOverall::Degraded
    } else if findings
        .values()
        .any(|finding| finding.status == InspectionStatus::Passed)
    {
        InspectionOverall::Healthy
    } else {
        InspectionOverall::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(side_effect: ProbeSideEffect) -> ProbeDefinition {
        ProbeDefinition {
            id: "probe".to_owned(),
            kind: "metadata".to_owned(),
            side_effect,
            default_enabled: false,
        }
    }

    #[test]
    fn billable_probe_requires_explicit_authorization() {
        assert!(
            authorize_probe(
                &probe(ProbeSideEffect::Billable),
                ProbeAuthorization::default()
            )
            .is_err()
        );
        assert!(
            authorize_probe(
                &probe(ProbeSideEffect::Billable),
                ProbeAuthorization {
                    allow_billable: true
                }
            )
            .is_ok()
        );
        assert!(
            authorize_probe(
                &probe(ProbeSideEffect::Mutating),
                ProbeAuthorization {
                    allow_billable: true
                }
            )
            .is_err()
        );
    }

    #[test]
    fn report_separates_declared_and_verified_context() -> Result<(), CoreError> {
        let report = InspectionReport::build(
            "provider",
            100,
            vec![InspectionFinding {
                dimension: InspectionDimension::ContextWindow,
                status: InspectionStatus::Partial,
                evidence: Evidence::ProviderDeclared,
                latency_ms: None,
                safe_summary: None,
                declared_value: Some("128k".to_owned()),
                verified_value: Some("32k".to_owned()),
            }],
            None,
            0,
        )?;
        assert_eq!(report.overall, InspectionOverall::Degraded);
        assert_ne!(
            report.findings[&InspectionDimension::ContextWindow].declared_value,
            report.findings[&InspectionDimension::ContextWindow].verified_value
        );
        Ok(())
    }

    #[test]
    fn schedule_enforces_pause_and_rate_limit() {
        let active = ProbeSchedule {
            paused: false,
            minimum_interval_ms: 1_000,
            last_run_at_unix_ms: Some(5_000),
        };
        assert!(active.allow_at(5_999).is_err());
        assert!(active.allow_at(6_000).is_ok());
        assert!(
            ProbeSchedule {
                paused: true,
                ..active
            }
            .allow_at(7_000)
            .is_err()
        );
    }
}
