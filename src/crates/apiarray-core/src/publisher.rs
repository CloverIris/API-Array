use crate::{CoreError, ErrorCode, SCHEMA_VERSION, ValidationIssue};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublisherConfig {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default = "default_listen_address")]
    pub listen_address: IpAddr,
    pub port: u16,
    #[serde(default = "default_path")]
    pub base_path: String,
    #[serde(default = "default_true")]
    pub require_token: bool,
    pub token_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublisherSummary {
    pub id: String,
    pub base_url: String,
    pub loopback_only: bool,
    pub authentication_enabled: bool,
}

impl PublisherConfig {
    /// 校验 Publisher 的回环监听、端口、路径与 Secret 引用。
    ///
    /// # Errors
    ///
    /// 配置尝试绑定非回环地址、缺少令牌引用或包含无效字段时返回
    /// [`ErrorCode::PublisherInvalid`]。
    pub fn validate(&self) -> Result<PublisherSummary, CoreError> {
        let mut issues = Vec::new();
        if self.schema_version != SCHEMA_VERSION {
            issues.push(ValidationIssue::new(
                "schema_version",
                "UNSUPPORTED_SCHEMA",
                "Publisher Schema 版本不受支持",
            ));
        }
        if self.id.trim().is_empty() {
            issues.push(ValidationIssue::new(
                "id",
                "REQUIRED",
                "Publisher ID 不能为空",
            ));
        }
        if self.name.trim().is_empty() {
            issues.push(ValidationIssue::new(
                "name",
                "REQUIRED",
                "Publisher 名称不能为空",
            ));
        }
        if !self.listen_address.is_loopback() {
            issues.push(ValidationIssue::new(
                "listen_address",
                "PUBLIC_BIND_FORBIDDEN",
                "MVP 只允许监听回环地址",
            ));
        }
        if self.port == 0 {
            issues.push(ValidationIssue::new(
                "port",
                "OUT_OF_RANGE",
                "端口必须大于 0",
            ));
        }
        if !self.base_path.starts_with('/')
            || self.base_path.contains("..")
            || self.base_path.contains('?')
            || self.base_path.contains('#')
        {
            issues.push(ValidationIssue::new(
                "base_path",
                "INVALID_PATH",
                "Base Path 必须是安全的绝对 URL Path",
            ));
        }
        if self.require_token {
            match self.token_ref.as_deref() {
                Some(value) if value.starts_with("secret://") => {}
                _ => issues.push(ValidationIssue::new(
                    "token_ref",
                    "SECRET_REFERENCE_REQUIRED",
                    "启用鉴权时必须使用 secret:// 引用",
                )),
            }
        }

        if issues.is_empty() {
            let host = match self.listen_address {
                IpAddr::V4(address) => address.to_string(),
                IpAddr::V6(address) => format!("[{address}]"),
            };
            Ok(PublisherSummary {
                id: self.id.clone(),
                base_url: format!(
                    "http://{host}:{}{}",
                    self.port,
                    self.base_path.trim_end_matches('/')
                ),
                loopback_only: true,
                authentication_enabled: self.require_token,
            })
        } else {
            Err(CoreError::validation(
                ErrorCode::PublisherInvalid,
                "Publisher 配置校验失败",
                issues,
            ))
        }
    }
}

fn default_listen_address() -> IpAddr {
    IpAddr::from([127, 0, 0, 1])
}

fn default_path() -> String {
    "/v1".to_owned()
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_loopback_publisher() -> Result<(), CoreError> {
        let summary = PublisherConfig {
            schema_version: 1,
            id: "local-ai".to_owned(),
            name: "Local AI".to_owned(),
            listen_address: "127.0.0.1".parse().expect("valid address"),
            port: 6188,
            base_path: "/v1".to_owned(),
            require_token: true,
            token_ref: Some("secret://publisher/local-ai".to_owned()),
        }
        .validate()?;
        assert_eq!(summary.base_url, "http://127.0.0.1:6188/v1");
        Ok(())
    }

    #[test]
    fn rejects_public_bind() {
        let config = PublisherConfig {
            schema_version: 1,
            id: "public".to_owned(),
            name: "Public".to_owned(),
            listen_address: "0.0.0.0".parse().expect("valid address"),
            port: 6188,
            base_path: "/v1".to_owned(),
            require_token: true,
            token_ref: Some("secret://publisher/public".to_owned()),
        };
        let error = config.validate().expect_err("public bind must fail");
        assert!(
            error
                .issues
                .iter()
                .any(|issue| issue.code == "PUBLIC_BIND_FORBIDDEN")
        );
    }
}
