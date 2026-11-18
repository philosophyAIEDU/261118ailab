//! 답변 생성 AI 공급자 추상화: 로컬 Ollama 또는 (사용자가 선택·동의한) Gemini API.
//! 임베딩(의미 검색 색인)은 공급자와 관계없이 항상 로컬 Ollama로만 만든다 — 연구실 전체 문단을 외부로 보내지 않기 위해.
use crate::error::AppResult;
use crate::gemini::Gemini;
use crate::ollama::{ChatMessage, Ollama};
use serde::Serialize;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
pub struct UsageEvent {
    pub provider: String,
    pub model: String,
    /// 서버에 실제로 보낸 요청 수 (재시도 포함)
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub error: bool,
}

pub type UsageSink = Arc<dyn Fn(&UsageEvent) + Send + Sync>;
/// 분당 한도 대기 등 진행 안내
pub type NoticeSink = Arc<dyn Fn(&str) + Send + Sync>;

pub enum Backend {
    Ollama(Ollama),
    Gemini { client: Gemini, rpm: u32 },
}

pub struct Llm {
    pub backend: Backend,
    pub model: String,
    pub usage: Option<UsageSink>,
    pub notice: Option<NoticeSink>,
}

impl Llm {
    pub fn ollama(client: Ollama, model: impl Into<String>) -> Self {
        Self { backend: Backend::Ollama(client), model: model.into(), usage: None, notice: None }
    }
    pub fn gemini(client: Gemini, model: impl Into<String>, rpm: u32) -> Self {
        Self { backend: Backend::Gemini { client, rpm }, model: model.into(), usage: None, notice: None }
    }
    pub fn is_cloud(&self) -> bool {
        matches!(self.backend, Backend::Gemini { .. })
    }
    pub fn provider(&self) -> &'static str {
        if self.is_cloud() { "gemini" } else { "ollama" }
    }
    /// 결과에 기록할 모델 이름 (클라우드는 구분 표시)
    pub fn label(&self) -> String {
        if self.is_cloud() { format!("{} (Gemini API)", self.model) } else { self.model.clone() }
    }

    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        temperature: f32,
        num_ctx: Option<u32>,
        json_format: bool,
        cancel: Arc<AtomicBool>,
        on_token: impl FnMut(&str) + Send,
    ) -> AppResult<String> {
        match &self.backend {
            Backend::Ollama(o) => o.chat(&self.model, messages, temperature, num_ctx, json_format, cancel, on_token).await,
            Backend::Gemini { client, rpm } => {
                let requests = std::sync::atomic::AtomicU64::new(0);
                let notice = self.notice.clone();
                let r = client
                    .chat(
                        &self.model,
                        messages,
                        temperature,
                        json_format,
                        *rpm,
                        cancel,
                        on_token,
                        || {
                            requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        },
                        move |secs| {
                            if let Some(n) = &notice {
                                n(&format!("Gemini 분당 사용 한도 때문에 {secs}초 기다리는 중…"));
                            }
                        },
                    )
                    .await;
                let n = requests.load(std::sync::atomic::Ordering::Relaxed);
                if let Some(sink) = &self.usage {
                    if n > 0 {
                        let (i, o) = r.as_ref().map(|(_, u)| (u.input_tokens, u.output_tokens)).unwrap_or((0, 0));
                        sink(&UsageEvent { provider: "gemini".into(), model: self.model.clone(), requests: n, input_tokens: i, output_tokens: o, error: r.is_err() });
                    }
                }
                r.map(|(t, _)| t)
            }
        }
    }
}
