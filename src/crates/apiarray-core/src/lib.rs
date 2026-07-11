//! API ARRAY 的独立业务核心。
//!
//! 这个 crate 不依赖 Tauri 或任何 UI 运行环境，可以被 CLI、桌面宿主和未来的
//! 无头 Runtime 复用。

pub mod adapter;
pub mod catalog;
pub mod canonical;
pub mod capability;
pub mod cli;
pub mod error;
pub mod events;
pub mod graph;
pub mod health;
pub mod inspection;
pub mod openai;
pub mod provider;
pub mod publisher;
pub mod routing;
pub mod runtime;
pub mod secret;
pub mod stream;
pub mod templates;
pub mod workspace;

pub use error::{CoreError, ErrorCode, ValidationIssue};

/// 当前 Core 所支持的公共 Schema 版本。
pub const SCHEMA_VERSION: u32 = 1;
