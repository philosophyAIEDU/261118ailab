//! 연구실 단위 설정 (DB에 저장되므로 백업·복원·업데이트 후에도 유지된다).
use crate::error::AppResult;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// 답변 생성 모델 (Ollama 모델 이름)
    pub chat_model: Option<String>,
    /// 임베딩(의미 검색) 모델
    pub embed_model: Option<String>,
    /// 선택적 재정렬 모델 (현재는 RRF 재정렬만 사용; 향후 확장용)
    pub rerank_model: Option<String>,
    pub ollama_port: u16,
    /// stable / beta
    pub update_channel: String,
    pub auto_check_updates: bool,
    /// 클라우드 AI 사용 여부 — 현재 버전에서는 항상 false (기능 없음)
    pub cloud_ai_enabled: bool,
    pub top_k: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            chat_model: None,
            embed_model: None,
            rerank_model: None,
            ollama_port: crate::ollama::DEFAULT_PORT,
            update_channel: "stable".into(),
            auto_check_updates: true,
            cloud_ai_enabled: false,
            top_k: 8,
        }
    }
}

const KEY: &str = "app_settings";

pub fn load(conn: &Connection) -> AppResult<Settings> {
    let v: Option<String> = conn.query_row("SELECT value FROM settings WHERE key=?1", [KEY], |r| r.get(0)).optional()?;
    let mut s: Settings = v.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default();
    s.cloud_ai_enabled = false;
    if s.update_channel != "beta" {
        s.update_channel = "stable".into();
    }
    Ok(s)
}

pub fn save(conn: &Connection, s: &Settings) -> AppResult<Settings> {
    let mut s = s.clone();
    s.cloud_ai_enabled = false; // 외부 전송 기능은 이 버전에 없다
    s.top_k = s.top_k.clamp(3, 15);
    if s.ollama_port == 0 {
        s.ollama_port = crate::ollama::DEFAULT_PORT;
    }
    conn.execute("INSERT INTO settings(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![KEY, serde_json::to_string(&s)?])?;
    Ok(s)
}

pub fn get_raw(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    Ok(conn.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| r.get(0)).optional()?)
}

pub fn set_raw(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute("INSERT INTO settings(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value])?;
    Ok(())
}
