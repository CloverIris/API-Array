use apiarray_core::runtime::RuntimeConfig;
use apiarray_runtime::env::{expand_env_placeholders, load_dotenv_optional};
use apiarray_runtime::publisher::{PublisherServer, PublisherState};
use apiarray_runtime::resilience::JsonlAuditSink;
use apiarray_runtime::secret::EnvironmentSecretResolver;
use apiarray_runtime::transport::{HttpExecutor, TransportConfig};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::{self, BufRead};
use std::process::ExitCode;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
struct LaunchRequest {
    schema_version: u32,
    publisher_id: String,
    runtime: RuntimeConfig,
    #[serde(default)]
    secret_env: BTreeMap<String, String>,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error runtime.launch {message}");
            ExitCode::from(1)
        }
    }
}

async fn run() -> Result<(), String> {
    load_dotenv_optional().map_err(|_| "DOTENV_LOAD_FAILED".to_owned())?;
    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|_| "STDIN_READ_FAILED".to_owned())?;
    if line.trim().is_empty() {
        return Err("EMPTY_LAUNCH_REQUEST".to_owned());
    }
    let expanded = expand_env_placeholders(&line).map_err(|_| "ENV_EXPANSION_FAILED".to_owned())?;
    let launch: LaunchRequest =
        serde_json::from_str(&expanded).map_err(|_| "INVALID_LAUNCH_JSON".to_owned())?;
    if launch.schema_version != 1 {
        return Err("UNSUPPORTED_SCHEMA".to_owned());
    }
    let runtime = Arc::new(
        launch
            .runtime
            .compile()
            .map_err(|_| "RUNTIME_COMPILE_FAILED".to_owned())?,
    );
    let executor = HttpExecutor::new(TransportConfig::default())
        .map_err(|_| "HTTP_CLIENT_INIT_FAILED".to_owned())?;
    let secrets = Arc::new(EnvironmentSecretResolver::new(launch.secret_env));
    let audit = Arc::new(
        JsonlAuditSink::open("runtime-audit.jsonl").map_err(|_| "AUDIT_OPEN_FAILED".to_owned())?,
    );
    let state = PublisherState::with_audit(runtime, &launch.publisher_id, executor, secrets, audit);
    let server = PublisherServer::bind(state)
        .await
        .map_err(|_| "PUBLISHER_BIND_FAILED".to_owned())?;
    eprintln!(
        "info runtime.ready publisher={} address={}",
        launch.publisher_id,
        server.local_address()
    );
    server
        .serve_with_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|_| "PUBLISHER_SERVE_FAILED".to_owned())?;
    eprintln!("info runtime.stopped publisher={}", launch.publisher_id);
    Ok(())
}
