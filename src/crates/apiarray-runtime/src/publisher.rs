use crate::secret::SecretResolver;
use crate::transport::HttpExecutor;
use crate::{RuntimeError, RuntimeErrorCode};
use apiarray_core::adapter::parse_response;
use apiarray_core::openai::{
    OpenAiStreamEncoder, encode_chat_completions_response, parse_chat_completions_request,
};
use apiarray_core::routing::HealthStatus;
use apiarray_core::runtime::{CompiledRuntime, DispatchRequest};
use apiarray_core::stream::{SseByteDecoder, StreamEvent, parse_protocol_frame};
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, State, rejection::JsonRejection};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{RwLock, mpsc};
use tokio_stream::wrappers::ReceiverStream;

#[derive(Clone)]
pub struct PublisherState {
    runtime: Arc<CompiledRuntime>,
    publisher_id: String,
    executor: HttpExecutor,
    secrets: Arc<dyn SecretResolver>,
    health: Arc<RwLock<BTreeMap<String, HealthStatus>>>,
}

impl PublisherState {
    #[must_use]
    pub fn new(
        runtime: Arc<CompiledRuntime>,
        publisher_id: impl Into<String>,
        executor: HttpExecutor,
        secrets: Arc<dyn SecretResolver>,
    ) -> Self {
        Self {
            runtime,
            publisher_id: publisher_id.into(),
            executor,
            secrets,
            health: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub async fn set_health(&self, upstream_id: impl Into<String>, status: HealthStatus) {
        self.health.write().await.insert(upstream_id.into(), status);
    }

    pub fn router(self) -> Router {
        Router::new()
            .route("/v1/chat/completions", post(chat_completions))
            .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
            .with_state(self)
    }
}

pub struct PublisherServer {
    listener: TcpListener,
    router: Router,
    local_address: SocketAddr,
}

impl PublisherServer {
    /// 按冻结安全策略绑定本地 Publisher。
    ///
    /// # Errors
    ///
    /// Publisher 不存在、配置无效或本地端口绑定失败时返回错误。
    pub async fn bind(state: PublisherState) -> Result<Self, RuntimeError> {
        let config = state
            .runtime
            .publisher_config(&state.publisher_id)
            .ok_or_else(|| {
                RuntimeError::new(RuntimeErrorCode::CoreRejected, "要启动的 Publisher 不存在")
            })?
            .clone();
        config.validate()?;
        let address = SocketAddr::new(config.listen_address, config.port);
        let listener = TcpListener::bind(address).await.map_err(|_| {
            RuntimeError::new(
                RuntimeErrorCode::PublisherBindFailed,
                format!("无法绑定本地 Publisher 地址 {address}"),
            )
        })?;
        let router = state.router();
        let local_address = listener.local_addr().map_err(|_| {
            RuntimeError::new(
                RuntimeErrorCode::PublisherBindFailed,
                "无法读取 Publisher 本地地址",
            )
        })?;
        Ok(Self {
            listener,
            router,
            local_address,
        })
    }

    #[must_use]
    pub const fn local_address(&self) -> SocketAddr {
        self.local_address
    }

    /// 运行 Publisher，直到 Server 失败或任务被取消。
    ///
    /// # Errors
    ///
    /// Axum Server 退出并返回错误时返回安全错误。
    pub async fn serve(self) -> Result<(), RuntimeError> {
        axum::serve(self.listener, self.router).await.map_err(|_| {
            RuntimeError::new(
                RuntimeErrorCode::PublisherServeFailed,
                "本地 Publisher Server 异常退出",
            )
        })
    }

    /// 运行 Publisher，并在 shutdown Future 完成时停止接受新连接。
    ///
    /// # Errors
    ///
    /// Axum Server 退出并返回错误时返回安全错误。
    pub async fn serve_with_shutdown<F>(self, shutdown: F) -> Result<(), RuntimeError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        axum::serve(self.listener, self.router)
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(|_| {
                RuntimeError::new(
                    RuntimeErrorCode::PublisherServeFailed,
                    "本地 Publisher Server 异常退出",
                )
            })
    }
}

async fn chat_completions(
    State(state): State<PublisherState>,
    headers: HeaderMap,
    payload: Result<Json<Value>, JsonRejection>,
) -> Response {
    if let Err(error) = authorize(&state, &headers) {
        return runtime_error_response(error);
    }
    let Ok(Json(payload)) = payload else {
        return openai_error_response(
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
            "请求正文不是有效 JSON",
        );
    };
    let request = match parse_chat_completions_request(&payload) {
        Ok(request) => request,
        Err(error) => return runtime_error_response(error.into()),
    };
    let health = state.health.read().await.clone();
    let plan = match state.runtime.plan_dispatch(&DispatchRequest {
        publisher_id: state.publisher_id.clone(),
        request: request.clone(),
        health,
        excluded_upstreams: HashSet::new(),
        previous_error: None,
    }) {
        Ok(plan) => plan,
        Err(error) => return runtime_error_response(error.into()),
    };

    if request.stream {
        match state
            .executor
            .execute_stream(&plan.transport, state.secrets.as_ref())
            .await
        {
            Ok(response) => stream_response(response, plan.transport.adapter, plan.public_model),
            Err(error) => runtime_error_response(error),
        }
    } else {
        match state
            .executor
            .execute_json(&plan.transport, state.secrets.as_ref())
            .await
            .and_then(|value| parse_response(plan.transport.adapter, &value).map_err(Into::into))
        {
            Ok(mut response) => {
                response.model = Some(plan.public_model);
                Json(encode_chat_completions_response(&response)).into_response()
            }
            Err(error) => runtime_error_response(error),
        }
    }
}

fn authorize(state: &PublisherState, headers: &HeaderMap) -> Result<(), RuntimeError> {
    let config = state
        .runtime
        .publisher_config(&state.publisher_id)
        .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::CoreRejected, "Publisher 不存在"))?;
    if !config.require_token {
        return Ok(());
    }
    let reference = config.token_ref.as_ref().ok_or_else(|| {
        RuntimeError::new(
            RuntimeErrorCode::SecretUnavailable,
            "Publisher Token 引用缺失",
        )
    })?;
    let expected = state.secrets.resolve(reference)?;
    let provided = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::PublisherUnauthorized,
                "缺少 Publisher Bearer Token",
            )
        })?;
    if !constant_time_equal(provided.as_bytes(), expected.expose().as_bytes()) {
        return Err(RuntimeError::new(
            RuntimeErrorCode::PublisherUnauthorized,
            "Publisher Bearer Token 无效",
        ));
    }
    Ok(())
}

fn stream_response(
    response: reqwest::Response,
    adapter: apiarray_core::adapter::AdapterKind,
    public_model: String,
) -> Response {
    let (sender, receiver) = mpsc::channel::<Result<Bytes, Infallible>>(32);
    tokio::spawn(async move {
        let mut upstream = response.bytes_stream();
        let mut decoder = SseByteDecoder::new();
        let mut encoder = OpenAiStreamEncoder::new("apiarray-stream", &public_model);
        let mut finished = false;
        while let Some(chunk) = upstream.next().await {
            let result = match chunk {
                Ok(bytes) => decoder.push(&bytes),
                Err(_) => Err(apiarray_core::CoreError::new(
                    apiarray_core::ErrorCode::StreamInvalid,
                    "上游流读取失败",
                )),
            };
            let Ok(frames) = result else {
                send_stream_error(&sender).await;
                return;
            };
            for frame in frames {
                let Ok(events) = parse_protocol_frame(adapter, &frame) else {
                    send_stream_error(&sender).await;
                    return;
                };
                if !send_stream_events(&sender, &mut encoder, events, &public_model, &mut finished)
                    .await
                {
                    return;
                }
            }
        }
        if let Ok(Some(frame)) = decoder.finish()
            && let Ok(events) = parse_protocol_frame(adapter, &frame)
        {
            let _ = send_stream_events(&sender, &mut encoder, events, &public_model, &mut finished)
                .await;
        }
        if !finished {
            let _ = send_encoded(
                &sender,
                encoder.encode(&StreamEvent::Finished {
                    reason: apiarray_core::canonical::FinishReason::Stop,
                }),
            )
            .await;
        }
    });

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("x-accel-buffering", "no")
        .body(Body::from_stream(ReceiverStream::new(receiver)))
        .unwrap_or_else(|_| {
            openai_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_RUNTIME_ERROR",
                "无法创建流式响应",
            )
        })
}

async fn send_stream_events(
    sender: &mpsc::Sender<Result<Bytes, Infallible>>,
    encoder: &mut OpenAiStreamEncoder,
    events: Vec<StreamEvent>,
    public_model: &str,
    finished: &mut bool,
) -> bool {
    for event in events {
        if *finished {
            continue;
        }
        let event = match event {
            StreamEvent::Started { response_id, .. } => StreamEvent::Started {
                response_id,
                model: Some(public_model.to_owned()),
            },
            other => other,
        };
        if matches!(event, StreamEvent::Finished { .. }) {
            *finished = true;
        }
        if !send_encoded(sender, encoder.encode(&event)).await {
            return false;
        }
    }
    true
}

async fn send_encoded(
    sender: &mpsc::Sender<Result<Bytes, Infallible>>,
    frames: Vec<String>,
) -> bool {
    for frame in frames {
        if sender.send(Ok(Bytes::from(frame))).await.is_err() {
            return false;
        }
    }
    true
}

async fn send_stream_error(sender: &mpsc::Sender<Result<Bytes, Infallible>>) {
    let frame = format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"error": {"code": "STREAM_ERROR", "message": "上游流式响应中断"}})
    );
    let _ = sender.send(Ok(Bytes::from(frame))).await;
}

fn runtime_error_response(error: RuntimeError) -> Response {
    let RuntimeError {
        code, safe_message, ..
    } = error;
    let status = match code {
        RuntimeErrorCode::PublisherUnauthorized => StatusCode::UNAUTHORIZED,
        RuntimeErrorCode::CoreRejected => StatusCode::BAD_REQUEST,
        RuntimeErrorCode::SecretUnavailable
        | RuntimeErrorCode::EnvironmentInvalid
        | RuntimeErrorCode::TransportBuildFailed
        | RuntimeErrorCode::PublisherBindFailed
        | RuntimeErrorCode::PublisherServeFailed => StatusCode::INTERNAL_SERVER_ERROR,
        RuntimeErrorCode::UpstreamFailed
        | RuntimeErrorCode::ResponseTooLarge
        | RuntimeErrorCode::ResponseInvalid => StatusCode::BAD_GATEWAY,
    };
    openai_error_response(status, &format!("{code:?}"), &safe_message)
}

fn openai_error_response(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(json!({"error": {"code": code, "message": message, "type": "apiarray_error"}})),
    )
        .into_response()
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let length = left.len().max(right.len());
    for index in 0..length {
        let left_byte = left.get(index).copied().unwrap_or(0);
        let right_byte = right.get(index).copied().unwrap_or(0);
        difference |= usize::from(left_byte ^ right_byte);
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_comparison_checks_content_and_length() {
        assert!(constant_time_equal(b"same-token", b"same-token"));
        assert!(!constant_time_equal(b"same-token", b"same-tokem"));
        assert!(!constant_time_equal(b"short", b"shorter"));
    }
}
