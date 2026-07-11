use apiarray_core::workspace::WorkspacePackage;
use apiarray_runtime::persistence::WorkspaceRepository;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use std::process::ExitCode;

#[derive(Debug, Deserialize)]
struct Request {
    schema_version: u32,
    #[serde(flatten)]
    command: Command,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum Command {
    LoadWorkspace {
        root: String,
    },
    SaveWorkspace {
        root: String,
        workspace: Box<WorkspacePackage>,
    },
}

#[derive(Debug, Serialize)]
struct Response {
    schema_version: u32,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'static str>,
}

fn main() -> ExitCode {
    let mut line = String::new();
    if io::stdin().lock().read_line(&mut line).is_err() {
        return emit(&Response {
            schema_version: 1,
            ok: false,
            result: None,
            error: Some("STDIN_READ_FAILED"),
        });
    }
    let response = match serde_json::from_str::<Request>(&line) {
        Ok(request) if request.schema_version == 1 => execute(request),
        Ok(_) => Response {
            schema_version: 1,
            ok: false,
            result: None,
            error: Some("UNSUPPORTED_SCHEMA"),
        },
        Err(_) => Response {
            schema_version: 1,
            ok: false,
            result: None,
            error: Some("INVALID_REQUEST"),
        },
    };
    emit(&response)
}

fn execute(request: Request) -> Response {
    let result = match request.command {
        Command::LoadWorkspace { root } => {
            WorkspaceRepository::new(root).load().and_then(|recovery| {
                serde_json::to_value(recovery.loaded).map_err(|_| {
                    apiarray_runtime::RuntimeError::new(
                        apiarray_runtime::RuntimeErrorCode::WorkspaceStorageUnavailable,
                        "工作区状态无法序列化",
                    )
                })
            })
        }
        Command::SaveWorkspace { root, workspace } => WorkspaceRepository::new(root)
            .save(&workspace)
            .map(|()| json!({"saved": true})),
    };
    match result {
        Ok(result) => Response {
            schema_version: 1,
            ok: true,
            result: Some(result),
            error: None,
        },
        Err(error) => {
            eprintln!("error control.command {:?}", error.code);
            Response {
                schema_version: 1,
                ok: false,
                result: None,
                error: Some("CONTROL_COMMAND_FAILED"),
            }
        }
    }
}

fn emit(response: &Response) -> ExitCode {
    let success = response.ok;
    let mut stdout = io::stdout().lock();
    if serde_json::to_writer(&mut stdout, &response).is_err() || writeln!(stdout).is_err() {
        return ExitCode::from(3);
    }
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
