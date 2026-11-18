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
    /// 클라우드 AI 사용 여부 — 사용자가 Gemini를 선택하고 동의했을 때만 true (자동 계산)
    pub cloud_ai_enabled: bool,
    pub top_k: usize,
    /// 답변 생성 공급자: "ollama"(기본, 로컬) 또는 "gemini"(클라우드, 동의 필요)
    pub provider: String,
    pub gemini_model: String,
    /// Gemini 데이터 전송 안내에 동의한 시각 (없으면 Gemini 사용 불가)
    pub gemini_consent_at: Option<String>,
    /// 모델별 무료 티어 한도 (사용자가 AI Studio 값으로 수정 가능)
    pub gemini_limits: std::collections::BTreeMap<String, crate::gemini::Limits>,
}

pub fn default_gemini_limits() -> std::collections::BTreeMap<String, crate::gemini::Limits> {
    crate::gemini::MODELS.iter().map(|(m, _)| (m.to_string(), crate::gemini::default_limits(m))).collect()
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
            provider: "ollama".into(),
            gemini_model: "gemini-3.5-flash-lite".into(),
            gemini_consent_at: None,
            gemini_limits: default_gemini_limits(),
        }
    }
}

const KEY: &str = "app_settings";

pub fn load(conn: &Connection) -> AppResult<Settings> {
    let v: Option<String> = conn.query_row("SELECT value FROM settings WHERE key=?1", [KEY], |r| r.get(0)).optional()?;
    let s: Settings = v.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default();
    Ok(normalize(s))
}

fn normalize(mut s: Settings) -> Settings {
    if s.update_channel != "beta" {
        s.update_channel = "stable".into();
    }
    if s.provider != "gemini" || s.gemini_consent_at.is_none() {
        s.provider = "ollama".into();
    }
    if !crate::gemini::MODELS.iter().any(|m| m.0 == s.gemini_model) {
        s.gemini_model = crate::gemini::MODELS[0].0.into();
    }
    for (m, l) in default_gemini_limits() {
        s.gemini_limits.entry(m).or_insert(l);
    }
    s.cloud_ai_enabled = s.provider == "gemini";
    s
}

pub fn save(conn: &Connection, s: &Settings) -> AppResult<Settings> {
    let mut s = s.clone();
    if s.provider == "gemini" && s.gemini_consent_at.is_none() {
        return Err(crate::error::AppError::Invalid("Gemini를 사용하려면 먼저 데이터 전송 안내를 확인하고 동의해 주세요".into()));
    }
    s = normalize(s);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::lib;

    #[test]
    fn gemini_requires_consent_and_defaults_are_local() {
        let (_d, _p, c) = lib();
        let s = load(&c).unwrap();
        assert_eq!(s.provider, "ollama");
        assert!(!s.cloud_ai_enabled);
        assert_eq!(s.gemini_limits["gemini-3.8-flash"].rpd, 20);
        let mut g = s.clone();
        g.provider = "gemini".into();
        assert!(save(&c, &g).is_err(), "동의 없이 Gemini 불가");
        g.gemini_consent_at = Some("2026-11-18T00:00:00Z".into());
        g.gemini_model = "unknown-model".into();
        let saved = save(&c, &g).unwrap();
        assert!(saved.cloud_ai_enabled);
        assert_eq!(saved.gemini_model, "gemini-3.5-flash-lite");
        // 예전 설정(필드 없음)도 읽힌다
        set_raw(&c, "app_settings", r#"{"chat_model":"qwen3:8b","ollama_port":11434}"#).unwrap();
        let old = load(&c).unwrap();
        assert_eq!(old.chat_model.as_deref(), Some("qwen3:8b"));
        assert_eq!(old.provider, "ollama");
    }
}
