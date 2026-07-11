use apiarray_core::adapter::{
    AdapterKind, HeaderPlan, HeaderValue as PlannedValue, HttpMethod, StreamProtocol, TransportPlan,
};
use apiarray_core::secret::SecretRef;
use apiarray_core::stream::SseByteDecoder;
use apiarray_runtime::env::load_dotenv_optional;
use apiarray_runtime::secret::EnvironmentSecretResolver;
use apiarray_runtime::transport::{HttpExecutor, TransportConfig};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::process::ExitCode;
use zeroize::Zeroizing;

const UPSTREAM_REF: &str = "secret://workspace/local/api-key";

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(code) => {
            eprintln!("error probe.failed code={code}");
            ExitCode::from(1)
        }
    }
}

async fn run() -> Result<Value, &'static str> {
    load_dotenv_optional().map_err(|_| "DOTENV_LOAD_FAILED")?;
    let operation = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "models".to_owned());
    match operation.as_str() {
        "models" => probe_models().await,
        "chat" => probe_local_chat(false).await,
        "stream" => probe_local_chat(true).await,
        _ => Err("UNKNOWN_OPERATION"),
    }
}

async fn probe_models() -> Result<Value, &'static str> {
    let base_url = required_env("APIARRAY_UPSTREAM_BASE_URL")?;
    let configured_model = required_env("APIARRAY_UPSTREAM_MODEL")?;
    let reference = SecretRef::parse(UPSTREAM_REF).map_err(|_| "SECRET_REF_INVALID")?;
    let resolver = EnvironmentSecretResolver::new(BTreeMap::from([(
        reference.as_str().to_owned(),
        "APIARRAY_UPSTREAM_API_KEY".to_owned(),
    )]));
    let plan = TransportPlan {
        adapter: AdapterKind::OpenaiCompatible,
        method: HttpMethod::Get,
        url: format!("{}/models", base_url.trim_end_matches('/')),
        headers: vec![HeaderPlan {
            name: "authorization".to_owned(),
            value: PlannedValue::SecretRef {
                reference,
                prefix: "Bearer ".to_owned(),
            },
        }],
        query: Vec::new(),
        body: Value::Null,
        stream_protocol: StreamProtocol::None,
    };
    let executor =
        HttpExecutor::new(TransportConfig::default()).map_err(|_| "HTTP_CLIENT_INIT_FAILED")?;
    let response = executor
        .execute_json(&plan, &resolver)
        .await
        .map_err(|_| "MODEL_DISCOVERY_FAILED")?;
    let models = response
        .get("data")
        .and_then(Value::as_array)
        .ok_or("MODEL_LIST_INVALID")?;
    let configured_model_found = models
        .iter()
        .any(|model| model.get("id").and_then(Value::as_str) == Some(configured_model.as_str()));
    Ok(json!({
        "ok": true,
        "operation": "models",
        "model_count": models.len(),
        "configured_model_found": configured_model_found,
    }))
}

async fn probe_local_chat(stream: bool) -> Result<Value, &'static str> {
    let token = Zeroizing::new(required_env("APIARRAY_PUBLISHER_TOKEN")?);
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .read_timeout(std::time::Duration::from_secs(90))
        .build()
        .map_err(|_| "LOCAL_CLIENT_INIT_FAILED")?;
    let response = client
        .post("http://127.0.0.1:6188/v1/chat/completions")
        .bearer_auth(token.as_str())
        .json(&json!({
            "model": "smart",
            "messages": [{"role": "user", "content": "Reply with only: OK"}],
            "max_tokens": 32,
            "temperature": 0,
            "stream": stream,
        }))
        .send()
        .await
        .map_err(|_| "LOCAL_REQUEST_FAILED")?;
    if !response.status().is_success() {
        return Err("LOCAL_UPSTREAM_REJECTED");
    }
    if stream {
        summarize_stream(response).await
    } else {
        let value: Value = response
            .json()
            .await
            .map_err(|_| "LOCAL_RESPONSE_INVALID")?;
        let content_length = value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(str::chars)
            .map_or(0, Iterator::count);
        Ok(json!({
            "ok": true,
            "operation": "chat",
            "has_choice": value.pointer("/choices/0").is_some(),
            "finish_reason": value.pointer("/choices/0/finish_reason").and_then(Value::as_str),
            "content_characters": content_length,
        }))
    }
}

async fn summarize_stream(response: reqwest::Response) -> Result<Value, &'static str> {
    let mut bytes = response.bytes_stream();
    let mut decoder = SseByteDecoder::new();
    let mut frame_count = 0_usize;
    let mut text_characters = 0_usize;
    let mut saw_done = false;
    while let Some(chunk) = bytes.next().await {
        let chunk = chunk.map_err(|_| "LOCAL_STREAM_READ_FAILED")?;
        for frame in decoder.push(&chunk).map_err(|_| "LOCAL_STREAM_INVALID")? {
            frame_count = frame_count.saturating_add(1);
            if frame.data.trim() == "[DONE]" {
                saw_done = true;
                continue;
            }
            if let Ok(value) = serde_json::from_str::<Value>(&frame.data)
                && let Some(text) = value
                    .pointer("/choices/0/delta/content")
                    .and_then(Value::as_str)
            {
                text_characters = text_characters.saturating_add(text.chars().count());
            }
        }
    }
    Ok(json!({
        "ok": true,
        "operation": "stream",
        "frame_count": frame_count,
        "text_characters": text_characters,
        "saw_done": saw_done,
    }))
}

fn required_env(name: &str) -> Result<String, &'static str> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or("ENV_VALUE_MISSING")
}
