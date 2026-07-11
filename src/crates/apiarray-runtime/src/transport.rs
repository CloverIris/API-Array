use crate::secret::SecretResolver;
use crate::{RuntimeError, RuntimeErrorCode};
use apiarray_core::adapter::{
    HeaderValue as PlannedValue, HttpMethod, TransportPlan, classify_http_error,
};
use apiarray_core::routing::StandardError;
use futures_util::StreamExt;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{Client, Request, Response};
use serde_json::Value;
use std::time::Duration;
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy)]
pub struct TransportConfig {
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub request_timeout: Duration,
    pub max_json_bytes: usize,
    pub max_error_bytes: usize,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(90),
            request_timeout: Duration::from_mins(2),
            max_json_bytes: 16 * 1024 * 1024,
            max_error_bytes: 256 * 1024,
        }
    }
}

#[derive(Clone)]
pub struct HttpExecutor {
    client: Client,
    config: TransportConfig,
}

impl HttpExecutor {
    /// 创建复用连接池的 HTTP 执行器。
    ///
    /// # Errors
    ///
    /// TLS 或 Client 配置无法初始化时返回错误。
    pub fn new(config: TransportConfig) -> Result<Self, RuntimeError> {
        let client = Client::builder()
            .connect_timeout(config.connect_timeout)
            .read_timeout(config.read_timeout)
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(8)
            .user_agent(concat!("API-ARRAY/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| {
                RuntimeError::new(
                    RuntimeErrorCode::TransportBuildFailed,
                    "HTTP Client 初始化失败",
                )
            })?;
        Ok(Self { client, config })
    }

    /// 执行非流式请求，并在大小限制内解析 JSON。
    ///
    /// # Errors
    ///
    /// Secret 无法解析、网络失败、上游非成功状态、响应过大或 JSON 无效时返回错误。
    pub async fn execute_json(
        &self,
        plan: &TransportPlan,
        secrets: &dyn SecretResolver,
    ) -> Result<Value, RuntimeError> {
        let mut request = self.prepare_request(plan, secrets)?;
        *request.timeout_mut() = Some(self.config.request_timeout);
        let response = self
            .client
            .execute(request)
            .await
            .map_err(map_reqwest_error)?;
        let response = self.require_success(response).await?;
        let bytes = read_limited(response, self.config.max_json_bytes).await?;
        serde_json::from_slice(&bytes).map_err(|_| {
            RuntimeError::new(RuntimeErrorCode::ResponseInvalid, "上游响应不是有效 JSON")
        })
    }

    /// 发起流式请求并在收到成功响应头后返回 Response。
    ///
    /// 调用方必须持续消费字节流；Client 的 read timeout 会检测停滞连接。
    ///
    /// # Errors
    ///
    /// Secret、网络或上游状态无效时返回错误。
    pub async fn execute_stream(
        &self,
        plan: &TransportPlan,
        secrets: &dyn SecretResolver,
    ) -> Result<Response, RuntimeError> {
        let request = self.prepare_request(plan, secrets)?;
        let response = self
            .client
            .execute(request)
            .await
            .map_err(map_reqwest_error)?;
        self.require_success(response).await
    }

    fn prepare_request(
        &self,
        plan: &TransportPlan,
        secrets: &dyn SecretResolver,
    ) -> Result<Request, RuntimeError> {
        let method = match plan.method {
            HttpMethod::Get => reqwest::Method::GET,
            HttpMethod::Post => reqwest::Method::POST,
        };
        let mut builder = self.client.request(method, &plan.url);
        for header in &plan.headers {
            let name = HeaderName::from_bytes(header.name.as_bytes()).map_err(|_| {
                RuntimeError::new(
                    RuntimeErrorCode::TransportBuildFailed,
                    "Provider 生成了无效 Header 名称",
                )
            })?;
            let value = resolve_value(&header.value, secrets)?;
            let value = HeaderValue::from_str(value.as_str()).map_err(|_| {
                RuntimeError::new(
                    RuntimeErrorCode::TransportBuildFailed,
                    "Provider 生成了无效 Header 值",
                )
            })?;
            builder = builder.header(name, value);
        }
        for query in &plan.query {
            let value = resolve_value(&query.value, secrets)?;
            builder = builder.query(&[(query.name.as_str(), value.as_str())]);
        }
        if plan.method == HttpMethod::Post {
            builder = builder.json(&plan.body);
        }
        builder.build().map_err(|_| {
            RuntimeError::new(
                RuntimeErrorCode::TransportBuildFailed,
                "无法构造上游 HTTP 请求",
            )
        })
    }

    async fn require_success(&self, response: Response) -> Result<Response, RuntimeError> {
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status().as_u16();
        let body = read_limited(response, self.config.max_error_bytes)
            .await
            .unwrap_or_default();
        let value = serde_json::from_slice(&body).unwrap_or_else(|_| {
            Value::String(String::from_utf8_lossy(&body).chars().take(240).collect())
        });
        let normalized = classify_http_error(status, &value);
        Err(RuntimeError::upstream(
            normalized.code,
            normalized.safe_message,
        ))
    }
}

fn resolve_value(
    planned: &PlannedValue,
    secrets: &dyn SecretResolver,
) -> Result<Zeroizing<String>, RuntimeError> {
    match planned {
        PlannedValue::Static { value } => Ok(Zeroizing::new(value.clone())),
        PlannedValue::SecretRef { reference, prefix } => {
            let secret = secrets.resolve(reference)?;
            let mut resolved = Zeroizing::new(String::with_capacity(
                prefix.len().saturating_add(secret.expose().len()),
            ));
            resolved.push_str(prefix);
            resolved.push_str(secret.expose());
            Ok(resolved)
        }
    }
}

async fn read_limited(response: Response, limit: usize) -> Result<Vec<u8>, RuntimeError> {
    let mut body = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(map_reqwest_error)?;
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(RuntimeError::new(
                RuntimeErrorCode::ResponseTooLarge,
                "上游响应超过本地安全大小限制",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn map_reqwest_error(error: reqwest::Error) -> RuntimeError {
    let standard = if error.is_timeout() {
        StandardError::ProviderTimeout
    } else if error.is_connect() {
        StandardError::NetworkUnreachable
    } else {
        StandardError::InvalidResponse
    };
    drop(error);
    RuntimeError::upstream(standard, "上游网络请求失败")
}

#[cfg(test)]
mod tests {
    use super::*;
    use apiarray_core::adapter::{AdapterKind, HeaderPlan, QueryPlan, StreamProtocol};
    use apiarray_core::secret::SecretRef;
    use std::collections::BTreeMap;

    struct TestResolver(BTreeMap<String, String>);

    impl SecretResolver for TestResolver {
        fn resolve(
            &self,
            reference: &SecretRef,
        ) -> Result<crate::secret::SecretValue, RuntimeError> {
            self.0
                .get(reference.as_str())
                .cloned()
                .map(crate::secret::SecretValue::new)
                .ok_or_else(|| {
                    RuntimeError::new(RuntimeErrorCode::SecretUnavailable, "missing test secret")
                })
        }
    }

    #[test]
    fn prepares_header_and_query_without_logging() -> Result<(), RuntimeError> {
        let reference = SecretRef::parse("secret://test/key")?;
        let plan = TransportPlan {
            adapter: AdapterKind::OpenaiCompatible,
            method: HttpMethod::Post,
            url: "https://example.invalid/v1/chat/completions".to_owned(),
            headers: vec![HeaderPlan {
                name: "authorization".to_owned(),
                value: PlannedValue::SecretRef {
                    reference: reference.clone(),
                    prefix: "Bearer ".to_owned(),
                },
            }],
            query: vec![QueryPlan {
                name: "key".to_owned(),
                value: PlannedValue::SecretRef {
                    reference: reference.clone(),
                    prefix: String::new(),
                },
            }],
            body: serde_json::json!({"model": "smart"}),
            stream_protocol: StreamProtocol::None,
        };
        let resolver = TestResolver(BTreeMap::from([(
            reference.as_str().to_owned(),
            "test-token".to_owned(),
        )]));
        let executor = HttpExecutor::new(TransportConfig::default())?;
        let request = executor.prepare_request(&plan, &resolver)?;
        assert_eq!(request.headers()["authorization"], "Bearer test-token");
        assert!(
            request
                .url()
                .query()
                .is_some_and(|query| query.contains("key="))
        );
        Ok(())
    }

    #[test]
    fn get_request_has_no_json_body() -> Result<(), RuntimeError> {
        let plan = TransportPlan {
            adapter: AdapterKind::OpenaiCompatible,
            method: HttpMethod::Get,
            url: "https://example.invalid/v1/models".to_owned(),
            headers: Vec::new(),
            query: Vec::new(),
            body: serde_json::Value::Null,
            stream_protocol: StreamProtocol::None,
        };
        let executor = HttpExecutor::new(TransportConfig::default())?;
        let request = executor.prepare_request(&plan, &TestResolver(BTreeMap::new()))?;
        assert!(request.body().is_none());
        Ok(())
    }
}
