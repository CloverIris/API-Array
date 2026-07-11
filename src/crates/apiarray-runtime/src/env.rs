use crate::{RuntimeError, RuntimeErrorCode};
use std::path::PathBuf;

/// 从当前目录或父目录加载首个 `.env`。已有进程环境变量优先。
///
/// # Errors
///
/// 找到 `.env` 但内容无法解析时返回错误；文件不存在不是错误。
pub fn load_dotenv_optional() -> Result<Option<PathBuf>, RuntimeError> {
    match dotenvy::dotenv() {
        Ok(path) => Ok(Some(path)),
        Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(RuntimeError::new(
            RuntimeErrorCode::EnvironmentInvalid,
            ".env 文件无法解析",
        )),
    }
}

/// 展开配置中的非敏感 `${VARIABLE}` 占位符。
///
/// API Key、Token、Password、Secret 和 Credential 变量禁止展开到配置文本中。
///
/// # Errors
///
/// 占位符无效、变量缺失或变量名称被判定为敏感时返回错误。
pub fn expand_env_placeholders(input: &str) -> Result<String, RuntimeError> {
    expand_placeholders_with(input, |name| std::env::var(name).ok())
}

fn expand_placeholders_with<F>(input: &str, mut lookup: F) -> Result<String, RuntimeError>
where
    F: FnMut(&str) -> Option<String>,
{
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while let Some(relative_start) = input[cursor..].find("${") {
        let start = cursor + relative_start;
        output.push_str(&input[cursor..start]);
        let name_start = start + 2;
        let relative_end = input[name_start..].find('}').ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::EnvironmentInvalid,
                "配置包含未闭合的环境变量占位符",
            )
        })?;
        let end = name_start + relative_end;
        let name = &input[name_start..end];
        if name.is_empty()
            || !name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            return Err(RuntimeError::new(
                RuntimeErrorCode::EnvironmentInvalid,
                "配置包含无效环境变量名称",
            ));
        }
        if is_sensitive_name(name) {
            return Err(RuntimeError::new(
                RuntimeErrorCode::EnvironmentInvalid,
                format!("敏感变量 {name} 只能通过 secret_env 引用，不能展开进配置"),
            ));
        }
        let value = lookup(name).ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::EnvironmentInvalid,
                format!("环境变量 {name} 未设置"),
            )
        })?;
        if value
            .chars()
            .any(|character| character == '"' || character == '\\' || character.is_control())
        {
            return Err(RuntimeError::new(
                RuntimeErrorCode::EnvironmentInvalid,
                format!("环境变量 {name} 包含不能安全写入 JSON 配置的字符"),
            ));
        }
        output.push_str(&value);
        cursor = end + 1;
    }
    output.push_str(&input[cursor..]);
    Ok(output)
}

fn is_sensitive_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ["KEY", "TOKEN", "SECRET", "PASSWORD", "CREDENTIAL"]
        .iter()
        .any(|marker| upper.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn expands_only_non_secret_values() -> Result<(), RuntimeError> {
        let values = BTreeMap::from([
            ("APIARRAY_BASE_URL", "https://example.invalid/v1"),
            ("APIARRAY_MODEL", "model-a"),
        ]);
        let result = expand_placeholders_with(
            r#"{"url":"${APIARRAY_BASE_URL}","model":"${APIARRAY_MODEL}"}"#,
            |name| values.get(name).map(ToString::to_string),
        )?;
        assert!(result.contains("https://example.invalid/v1"));
        assert!(result.contains("model-a"));
        Ok(())
    }

    #[test]
    fn refuses_to_expand_secret_variable() {
        let error = expand_placeholders_with("${APIARRAY_API_KEY}", |_| Some("value".to_owned()))
            .expect_err("secret expansion must fail");
        assert_eq!(error.code, RuntimeErrorCode::EnvironmentInvalid);
    }
}
