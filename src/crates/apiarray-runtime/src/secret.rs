use crate::{RuntimeError, RuntimeErrorCode};
use apiarray_core::secret::SecretRef;
use std::collections::BTreeMap;
use std::fmt::{self, Debug, Formatter};
use std::sync::{Arc, Mutex};
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

/// 受控 Secret 存储接口。工作区只保存引用，实际值永不进入导出文件。
pub trait SecretStore: Send + Sync {
    /// 保存或替换一个 Secret。
    ///
    /// # Errors
    ///
    /// 系统凭据库不可用或输入为空时返回错误。
    fn put(&self, reference: &SecretRef, value: SecretValue) -> Result<(), RuntimeError>;

    /// 按引用读取 Secret。
    ///
    /// # Errors
    ///
    /// 引用不存在或系统凭据库不可用时返回错误。
    fn get(&self, reference: &SecretRef) -> Result<SecretValue, RuntimeError>;

    /// 删除一个 Secret。
    ///
    /// # Errors
    ///
    /// 系统凭据库不可用时返回错误。
    fn delete(&self, reference: &SecretRef) -> Result<(), RuntimeError>;

    /// 判断当前用户的凭据库是否包含引用。
    fn contains(&self, reference: &SecretRef) -> bool;
}

/// 将一个 `SecretStore` 适配为 Runtime 的按需解析器。
#[derive(Clone)]
pub struct StoreSecretResolver {
    store: Arc<dyn SecretStore>,
}

impl StoreSecretResolver {
    #[must_use]
    pub fn new(store: Arc<dyn SecretStore>) -> Self {
        Self { store }
    }
}

impl SecretResolver for StoreSecretResolver {
    fn resolve(&self, reference: &SecretRef) -> Result<SecretValue, RuntimeError> {
        self.store.get(reference)
    }
}

/// Windows Credential Manager 存储。每个 `secret://` 引用对应当前 Windows 用户下的一项凭据。
#[derive(Debug, Clone)]
pub struct WindowsCredentialStore {
    service: String,
}

impl WindowsCredentialStore {
    #[must_use]
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, reference: &SecretRef) -> Result<keyring::v1::Entry, RuntimeError> {
        keyring::v1::Entry::new(&self.service, reference.as_str()).map_err(|_| {
            RuntimeError::new(
                RuntimeErrorCode::SecretStoreUnavailable,
                "无法访问 Windows Credential Manager",
            )
        })
    }
}

impl SecretStore for WindowsCredentialStore {
    fn put(&self, reference: &SecretRef, value: SecretValue) -> Result<(), RuntimeError> {
        if value.expose().is_empty() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::SecretStoreUnavailable,
                "不允许保存空 Secret",
            ));
        }
        self.entry(reference)?
            .set_password(value.expose())
            .map_err(|_| {
                RuntimeError::new(
                    RuntimeErrorCode::SecretStoreUnavailable,
                    "无法写入 Windows Credential Manager",
                )
            })
    }

    fn get(&self, reference: &SecretRef) -> Result<SecretValue, RuntimeError> {
        self.entry(reference)?
            .get_password()
            .map(SecretValue::new)
            .map_err(|_| {
                RuntimeError::new(
                    RuntimeErrorCode::SecretUnavailable,
                    "Windows Credential Manager 中不存在该 Secret",
                )
            })
    }

    fn delete(&self, reference: &SecretRef) -> Result<(), RuntimeError> {
        self.entry(reference)?.delete_credential().map_err(|_| {
            RuntimeError::new(
                RuntimeErrorCode::SecretStoreUnavailable,
                "无法删除 Windows Credential Manager Secret",
            )
        })
    }

    fn contains(&self, reference: &SecretRef) -> bool {
        self.get(reference).is_ok()
    }
}

/// 仅用于测试、CLI 纯逻辑和未来宿主依赖注入的内存 Secret 存储。
#[derive(Default)]
pub struct MemorySecretStore {
    values: Mutex<BTreeMap<String, SecretValue>>,
}

impl SecretStore for MemorySecretStore {
    fn put(&self, reference: &SecretRef, value: SecretValue) -> Result<(), RuntimeError> {
        if value.expose().is_empty() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::SecretStoreUnavailable,
                "不允许保存空 Secret",
            ));
        }
        self.values
            .lock()
            .map_err(|_| {
                RuntimeError::new(
                    RuntimeErrorCode::SecretStoreUnavailable,
                    "SecretStore 不可用",
                )
            })?
            .insert(reference.as_str().to_owned(), value);
        Ok(())
    }

    fn get(&self, reference: &SecretRef) -> Result<SecretValue, RuntimeError> {
        self.values
            .lock()
            .map_err(|_| {
                RuntimeError::new(
                    RuntimeErrorCode::SecretStoreUnavailable,
                    "SecretStore 不可用",
                )
            })?
            .get(reference.as_str())
            .map(|value| SecretValue::new(value.expose()))
            .ok_or_else(|| {
                RuntimeError::new(RuntimeErrorCode::SecretUnavailable, "Secret 引用尚未绑定")
            })
    }

    fn delete(&self, reference: &SecretRef) -> Result<(), RuntimeError> {
        self.values
            .lock()
            .map_err(|_| {
                RuntimeError::new(
                    RuntimeErrorCode::SecretStoreUnavailable,
                    "SecretStore 不可用",
                )
            })?
            .remove(reference.as_str());
        Ok(())
    }

    fn contains(&self, reference: &SecretRef) -> bool {
        self.values
            .lock()
            .is_ok_and(|values| values.contains_key(reference.as_str()))
    }
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

    #[test]
    fn store_resolver_reads_without_exposing_secret_in_debug() -> Result<(), RuntimeError> {
        let reference = SecretRef::parse("secret://workspace/example/api-key")?;
        let store = Arc::new(MemorySecretStore::default());
        store.put(&reference, SecretValue::new("memory-secret-value"))?;
        let resolver = StoreSecretResolver::new(store);
        assert_eq!(
            resolver.resolve(&reference)?.expose(),
            "memory-secret-value"
        );
        assert!(!format!("{:?}", resolver.resolve(&reference)?).contains("memory-secret"));
        Ok(())
    }
}
