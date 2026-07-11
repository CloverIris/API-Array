use crate::{CoreError, ErrorCode};
use serde::{Deserialize, Serialize};

const SECRET_PREFIX: &str = "secret://";

/// 工作区中可序列化的 Secret 引用。它永远不保存 Secret 值。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretRef(String);

impl SecretRef {
    /// 解析并校验一个不包含 Secret 值的引用。
    ///
    /// # Errors
    ///
    /// 当值不以 `secret://` 开头或标识符包含非法字符时返回
    /// [`ErrorCode::SecretInvalid`]。
    pub fn parse(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        let identifier = value.strip_prefix(SECRET_PREFIX).ok_or_else(|| {
            CoreError::new(ErrorCode::SecretInvalid, "Secret 引用必须以 secret:// 开头")
        })?;

        if identifier.is_empty()
            || identifier.starts_with('/')
            || identifier.ends_with('/')
            || !identifier
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_/.:".contains(character))
        {
            return Err(CoreError::new(
                ErrorCode::SecretInvalid,
                "Secret 引用包含非法或空的标识符",
            ));
        }

        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 仅用于诊断显示。保留最多四个尾字符，不应被用作认证或持久化。
#[must_use]
pub fn redact_secret(value: &str) -> String {
    if value.is_empty() {
        return "***".to_owned();
    }

    let suffix: String = value
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("***{suffix}")
}

/// 对常见鉴权文本做保守脱敏。未知文本保持原样。
#[must_use]
pub fn redact_diagnostic(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("authorization")
        || lower.contains("api_key")
        || lower.contains("api-key")
        || lower.contains("apikey")
        || lower.contains("bearer ")
    {
        "[REDACTED]".to_owned()
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_secret_reference() -> Result<(), CoreError> {
        let reference = SecretRef::parse("secret://workspace/openai/main")?;
        assert_eq!(reference.as_str(), "secret://workspace/openai/main");
        Ok(())
    }

    #[test]
    fn rejects_plain_secret_value() {
        assert!(SecretRef::parse("sk-not-a-reference").is_err());
    }

    #[test]
    fn redaction_never_returns_the_whole_value() {
        assert_eq!(redact_secret("sk-12345678"), "***5678");
        assert_eq!(redact_diagnostic("Authorization: Bearer abc"), "[REDACTED]");
    }
}
