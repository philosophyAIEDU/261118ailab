//! Philosophy Research Hub (교수님만의 AI 연구실) 핵심 로직.
//!
//! 이 크레이트는 Tauri에 의존하지 않는다. 모든 연구자료 처리는 로컬에서 이루어지며,
//! 네트워크는 로컬 AI 엔진(Ollama, 127.0.0.1)에만 연결한다.
pub mod annotations;
pub mod backup;
pub mod bib;
pub mod compare;
pub mod db;
pub mod demo;
pub mod error;
pub mod export;
pub mod extract;
pub mod graph;
pub mod library;
pub mod metadata;
pub mod models;
pub mod notes;
pub mod ollama;
pub mod paths;
pub mod projects;
pub mod rag;
pub mod review;
pub mod search;
pub mod settings;
pub mod text;

pub use error::{AppError, AppResult};
pub use rusqlite;

/// 현재 시각 (UTC, RFC3339)
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
