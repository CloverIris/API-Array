//! API ARRAY 的异步数据面。
//!
//! 本 crate 负责真实 HTTP 传输与本地 Publisher，但不依赖 Tauri 或 UI。

pub mod env;
pub mod error;
pub mod publisher;
pub mod resilience;
pub mod secret;
pub mod transport;

pub use error::{RuntimeError, RuntimeErrorCode};
