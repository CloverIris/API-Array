use crate::{RuntimeError, RuntimeErrorCode};
use apiarray_core::secret::SecretRef;
use std::collections::BTreeMap;
use std::fmt::{self, Debug, Formatter};
use zeroize::Zeroizing;

pub struct SecretValue(Zeroizing<String>);

impl SecretValue {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl Debug for SecretValue {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretValue([REDACTED])")
    }
}

pub trait SecretResolver: Send + Sync {
    /// 解析 Secret 引用。实现不得记录返回值。
    ///
    /// # Errors
    ///
    /// 引用未绑定或底层凭据存储不可用时返回错误。
    fn resolve(&self, reference: &SecretRef) -> Result<SecretValue, RuntimeError>;
}

/// 将 Secret 引用映射到环境变量名。它不读取 `.env` 文件，也不缓存明文值。
#[derive(Debug, Clone, Default)]
pub struct EnvironmentSecretResolver {
    bindings: BTreeMap<String, String>,
}

impl EnvironmentSecretResolver {
    #[must_use]
    pub fn new(bindings: BTreeMap<String, String>) -> Self {
        Self { bindings }
    }
}

impl SecretResolver for EnvironmentSecretResolver {
    fn resolve(&self, reference: &SecretRef) -> Result<SecretValue, RuntimeError> {
        let variable = self.bindings.get(reference.as_str()).ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::SecretUnavailable,
                "Secret 引用尚未绑定到环境变量",
            )
        })?;
        let value = std::env::var(variable).map_err(|_| {
            RuntimeError::new(
                RuntimeErrorCode::SecretUnavailable,
                format!("Secret 环境变量 {variable} 不存在"),
            )
        })?;
        if value.is_empty() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::SecretUnavailable,
                "Secret 环境变量不能为空",
            ));
        }
        Ok(SecretValue::new(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_exposes_secret() {
        let secret = SecretValue::new("sensitive-value");
        assert_eq!(format!("{secret:?}"), "SecretValue([REDACTED])");
        assert!(!format!("{secret:?}").contains("sensitive"));
    }
}
