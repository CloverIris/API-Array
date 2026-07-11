use crate::resilience::{AuditSink, ExecutedStream, NoopAuditSink, ResilientExecutor, TraceResult};
use crate::secret::SecretResolver;
use crate::transport::HttpExecutor;
use crate::{RuntimeError, RuntimeErrorCode};
use apiarray_core::openai::{
    OpenAiStreamEncoder, encode_chat_completions_response, parse_chat_completions_request,
};
use apiarray_core::routing::{HealthStatus, StandardError};
use apiarray_core::runtime::CompiledRuntime;
use apiarray_core::stream::{SseByteDecoder, StreamEvent, parse_protocol_frame};
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, State, rejection::JsonRejection};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

#[derive(Clone)]
pub struct PublisherState {
    runtime: Arc<CompiledRuntime>,
    publisher_id: String,
    secrets: Arc<dyn SecretResolver>,
    executor: ResilientExecutor,
}

impl PublisherState {
    #[must_use]
    pub fn new(
        runtime: Arc<CompiledRuntime>,
        publisher_id: impl Into<String>,
        executor: HttpExecutor,
        secrets: Arc<dyn SecretResolver>,
    ) -> Self {
        Self::with_audit(
            runtime,
            publisher_id,
            executor,
            secrets,
            Arc::new(NoopAuditSink),
        )
    }

    #[must_use]
    pub fn with_audit(
        runtime: Arc<CompiledRuntime>,
        publisher_id: impl Into<String>,
        transport: HttpExecutor,
        secrets: Arc<dyn SecretResolver>,
        audit: Arc<dyn AuditSink>,
    ) -> Self {
        Self {
            runtime,
            publisher_id: publisher_id.into(),
            executor: ResilientExecutor::new(transport, Arc::clone(&secrets), audit),
            secrets,
        }
    }

    pub async fn set_health(&self, upstream_id: impl Into<String>, status: HealthStatus) {
        self.executor.set_health(upstream_id, status).await;
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
    /// 只按已验证的本地 Publisher 配置绑定监听地址。
    ///
    /// # Errors
    ///
    /// Publisher 不存在、配置无效或端口绑定失败时返回错误。
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

    /// 运行 Publisher，直到 Server 退出。
    ///
    /// # Errors
    ///
    /// Server 异常退出时返回安全错误。
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
    /// Server 异常退出时返回安全错误。
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
    let request_id = correlation_id(&headers);
    if let Err(error) = authorize(&state, &headers) {
        return with_correlation_id(runtime_error_response(error), &request_id);
    }
    let Ok(Json(payload)) = payload else {
        return with_correlation_id(
            openai_error_response(
                StatusCode::BAD_REQUEST,
                "INVALID_REQUEST",
                "请求正文不是有效 JSON",
            ),
            &request_id,
        );
    };
    let request = match parse_chat_completions_request(&payload) {
        Ok(request) => request,
        Err(error) => {
            return with_correlation_id(runtime_error_response(error.into()), &request_id);
        }
    };

    if request.stream {
        match state
            .executor
            .execute_stream(
                &state.runtime,
                &state.publisher_id,
                request,
                request_id.clone(),
            )
            .await
        {
            Ok(executed) => with_correlation_id(
                stream_response(executed, state.executor.clone()),
                &request_id,
            ),
            Err(error) => with_correlation_id(runtime_error_response(error), &request_id),
        }
    } else {
        match state
            .executor
            .execute_json(
                &state.runtime,
                &state.publisher_id,
                request,
                request_id.clone(),
            )
            .await
        {
            Ok(mut executed) => {
                executed.response.model = Some(executed.public_model);
                with_correlation_id(
                    Json(encode_chat_completions_response(&executed.response)).into_response(),
                    &request_id,
                )
            }
            Err(error) => with_correlation_id(runtime_error_response(error), &request_id),
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

#[allow(clippy::too_many_lines)]
fn stream_response(executed: ExecutedStream, executor: ResilientExecutor) -> Response {
    let ExecutedStream {
        response,
        adapter,
        public_model,
        mut trace,
    } = executed;
    let (sender, receiver) = mpsc::channel::<Result<Bytes, Infallible>>(32);
    tokio::spawn(async move {
        let stream_started = Instant::now();
        let mut upstream = response.bytes_stream();
        let mut decoder = SseByteDecoder::new();
        let mut encoder = OpenAiStreamEncoder::new("apiarray-stream", &public_model);
        let mut finished = false;
        while let Some(chunk) = upstream.next().await {
            let frames = match chunk {
                Ok(bytes) => decoder.push(&bytes),
                Err(_) => Err(apiarray_core::CoreError::new(
                    apiarray_core::ErrorCode::StreamInvalid,
                    "上游流读取失败",
                )),
            };
            let Ok(frames) = frames else {
                send_stream_error(&sender).await;
                finish_trace(&executor, &mut trace, stream_started, TraceResult::Failure);
                return;
            };
            for frame in frames {
                let Ok(events) = parse_protocol_frame(adapter, &frame) else {
                    send_stream_error(&sender).await;
                    finish_trace(&executor, &mut trace, stream_started, TraceResult::Failure);
                    return;
                };
                if !send_stream_events(
                    &sender,
                    &mut encoder,
                    events,
                    &public_model,
                    &mut finished,
                    &mut trace,
                )
                .await
                {
                    finish_trace(
                        &executor,
                        &mut trace,
                        stream_started,
                        TraceResult::ClientDisconnected,
                    );
                    return;
                }
            }
        }
        match decoder.finish() {
            Ok(Some(frame)) => {
                let Ok(events) = parse_protocol_frame(adapter, &frame) else {
                    send_stream_error(&sender).await;
                    finish_trace(&executor, &mut trace, stream_started, TraceResult::Failure);
                    return;
                };
                if !send_stream_events(
                    &sender,
                    &mut encoder,
                    events,
                    &public_model,
                    &mut finished,
                    &mut trace,
                )
                .await
                {
                    finish_trace(
                        &executor,
                        &mut trace,
                        stream_started,
                        TraceResult::ClientDisconnected,
                    );
                    return;
                }
            }
            Ok(None) => {}
            Err(_) => {
                send_stream_error(&sender).await;
                finish_trace(&executor, &mut trace, stream_started, TraceResult::Failure);
                return;
            }
        }
        if !finished
            && !send_encoded(
                &sender,
                encoder.encode(&StreamEvent::Finished {
                    reason: apiarray_core::canonical::FinishReason::Stop,
                }),
            )
            .await
        {
            finish_trace(
                &executor,
                &mut trace,
                stream_started,
                TraceResult::ClientDisconnected,
            );
            return;
        }
        finish_trace(&executor, &mut trace, stream_started, TraceResult::Success);
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

fn finish_trace(
    executor: &ResilientExecutor,
    trace: &mut crate::resilience::ExecutionTrace,
    started: Instant,
    result: TraceResult,
) {
    trace.total_latency_ms = trace.total_latency_ms.saturating_add(elapsed_ms(started));
    trace.result = result;
    if result == TraceResult::Failure {
        trace.final_error = Some(StandardError::InvalidResponse);
    }
    executor.record(trace);
}

async fn send_stream_events(
    sender: &mpsc::Sender<Result<Bytes, Infallible>>,
    encoder: &mut OpenAiStreamEncoder,
    events: Vec<StreamEvent>,
    public_model: &str,
    finished: &mut bool,
    trace: &mut crate::resilience::ExecutionTrace,
) -> bool {
    for event in events {
        if *finished {
            continue;
        }
        if let StreamEvent::Usage { usage } = &event {
            trace.input_tokens = Some(usage.input_tokens);
            trace.output_tokens = Some(usage.output_tokens);
            trace.cached_input_tokens = usage.cached_input_tokens;
            trace.cache_hit = usage.cached_input_tokens.map(|tokens| tokens > 0);
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

fn correlation_id(headers: &HeaderMap) -> String {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte))
        })
        .map_or_else(
            || format!("apiarray-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed)),
            str::to_owned,
        )
}

fn with_correlation_id(mut response: Response, correlation_id: &str) -> Response {
    if let Ok(value) = correlation_id.parse() {
        response
            .headers_mut()
            .insert("x-apiarray-request-id", value);
    }
    response
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
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
        | RuntimeErrorCode::PublisherServeFailed
        | RuntimeErrorCode::AuditUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
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

    #[test]
    fn correlation_id_rejects_unsafe_input() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", "unsafe value".parse().expect("header"));
        assert!(correlation_id(&headers).starts_with("apiarray-"));
        headers.insert("x-request-id", "safe-id:1".parse().expect("header"));
        assert_eq!(correlation_id(&headers), "safe-id:1");
    }
}
