//! API ARRAY 的异步数据面。
//!
//! 本 crate 负责真实 HTTP 传输与本地 Publisher，但不依赖 Tauri 或 UI。

pub mod control;
pub mod env;
pub mod error;
pub mod inspection;
pub mod persistence;
pub mod publisher;
pub mod resilience;
pub mod secret;
pub mod supervisor;
pub mod transport;

pub use error::{RuntimeError, RuntimeErrorCode};
