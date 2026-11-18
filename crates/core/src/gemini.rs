//! Google Gemini API 클라이언트 (사용자가 명시적으로 선택·동의한 경우에만 사용).
//!
//! - 연결 주소는 `generativelanguage.googleapis.com`(HTTPS)로 고정한다. 시험용 주소 변경은 루프백만 허용한다.
//! - API 키는 이 모듈에 저장하지 않는다. 호출할 때 받아 `x-goog-api-key` 헤더로만 보낸다 (URL·로그에 남기지 않음).
//! - 무료 티어의 분당 요청 한도를 넘지 않도록 모델별로 요청 간격을 조절한다.
//! - 전송되는 내용: 질문과 검색된 관련 문단(또는 비교·초안에 필요한 문단). 논문 전체 파일은 보내지 않는다.
use crate::error::{AppError, AppResult};
use crate::ollama::ChatMessage;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub const API_HOST: &str = "https://generativelanguage.googleapis.com";

/// 이 앱에서 고를 수 있는 Gemini 모델 (사용자 요청 기준)
pub const MODELS: &[(&str, &str)] = &[
    ("gemini-3.5-flash-lite", "Gemini 3.5 Flash-Lite — 빠르고 하루 무료 사용량이 많음"),
    ("gemini-3.8-flash", "Gemini 3.8 Flash — 더 정확하지만 하루 무료 사용량이 적음"),
];

#[derive(Debug, Clone, Default, Serialize, serde::Deserialize, PartialEq)]
pub struct Limits {
    /// 분당 요청 수
    pub rpm: u32,
    /// 하루 요청 수 (태평양 시간 자정 초기화)
    pub rpd: u32,
}

/// 무료 티어 기본 한도 (2026-11 기준 공개 자료의 값, Google AI Studio에서 실제 값 확인 필요 — 설정에서 수정 가능)
pub fn default_limits(model: &str) -> Limits {
    match model {
        "gemini-3.8-flash" => Limits { rpm: 5, rpd: 20 },
        "gemini-3.5-flash-lite" => Limits { rpm: 10, rpd: 500 },
        _ => Limits { rpm: 5, rpd: 20 },
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Clone)]
pub struct Gemini {
    base: String,
    key: String,
    http: reqwest::Client,
}

/// 모델별 최근 1분간 요청 시각 (프로그램 전체 공유)
fn window() -> &'static Mutex<HashMap<String, VecDeque<Instant>>> {
    static W: OnceLock<Mutex<HashMap<String, VecDeque<Instant>>>> = OnceLock::new();
    W.get_or_init(Default::default)
}

/// 분당 한도를 넘지 않으려면 얼마나 기다려야 하는지 계산하고, 기다릴 필요가 없으면 지금 요청을 기록한다.
pub fn reserve_slot(model: &str, rpm: u32, now: Instant) -> Option<Duration> {
    if rpm == 0 {
        return None;
    }
    let mut w = window().lock().unwrap();
    let q = w.entry(model.to_string()).or_default();
    while q.front().map(|t| now.duration_since(*t) >= Duration::from_secs(60)).unwrap_or(false) {
        q.pop_front();
    }
    if q.len() as u32 >= rpm {
        let wait = Duration::from_secs(60) - now.duration_since(*q.front().unwrap()) + Duration::from_millis(250);
        return Some(wait);
    }
    q.push_back(now);
    None
}

pub fn mask_key(k: &str) -> String {
    let k = k.trim();
    let n = k.chars().count();
    if n <= 8 {
        return "●".repeat(n);
    }
    let head: String = k.chars().take(4).collect();
    let tail: String = k.chars().skip(n - 4).collect();
    format!("{head}…{tail}")
}

pub fn validate_key_format(k: &str) -> AppResult<String> {
    let k = k.trim().to_string();
    if k.len() < 20 || k.len() > 200 || k.chars().any(|c| c.is_whitespace() || !c.is_ascii_graphic()) {
        return Err(AppError::Invalid("API 키 형식이 올바르지 않습니다. Google AI Studio에서 복사한 키 전체를 붙여 넣어 주세요.".into()));
    }
    Ok(k)
}

/// Gemini 오류 응답을 한국어 안내로 바꾼다. (재시도 대기 시간이 있으면 함께 돌려준다)
pub fn translate_error(status: u16, body: &Value) -> (AppError, Option<Duration>, bool) {
    let e = &body["error"];
    let msg = e["message"].as_str().unwrap_or("");
    let st = e["status"].as_str().unwrap_or("");
    let details = e["details"].as_array().cloned().unwrap_or_default();
    let reason = details.iter().find_map(|d| d["reason"].as_str()).unwrap_or("");
    let retry = details.iter().find_map(|d| d["retryDelay"].as_str()).and_then(|s| s.trim_end_matches('s').parse::<f64>().ok()).map(|s| Duration::from_millis((s * 1000.0) as u64));
    let quota_ids: Vec<String> = details
        .iter()
        .flat_map(|d| d["violations"].as_array().cloned().unwrap_or_default())
        .filter_map(|v| v["quotaId"].as_str().map(String::from))
        .collect();
    let per_day = quota_ids.iter().any(|q| q.contains("PerDay")) || msg.to_lowercase().contains("per day");
    let short: String = msg.chars().take(160).collect();
    let err = match status {
        400 if reason == "API_KEY_INVALID" || msg.contains("API key not valid") => AppError::Ai("Gemini API 키가 올바르지 않습니다. 설정 > AI 모델에서 키를 다시 확인해 주세요.".into()),
        400 if msg.contains("location is not supported") || reason == "FAILED_PRECONDITION" => AppError::Ai("이 지역에서는 Gemini API 무료 티어를 사용할 수 없습니다. 로컬 AI를 사용해 주세요.".into()),
        401 | 403 => AppError::Ai("Gemini API 사용 권한이 없습니다. 키가 올바른지, AI Studio에서 API가 사용 설정되어 있는지 확인해 주세요.".into()),
        404 => AppError::Ai("선택한 Gemini 모델을 찾을 수 없습니다. 설정 > AI 모델의 '키 확인'으로 사용 가능한 모델을 확인해 주세요.".into()),
        429 if per_day => AppError::Ai("오늘의 Gemini 무료 사용량을 모두 사용했습니다. 태평양 시간 자정(한국 시간 오후 4~5시)에 초기화됩니다. 그동안 로컬 AI를 사용하거나 다른 Gemini 모델을 선택해 주세요.".into()),
        429 => AppError::Ai("Gemini 분당 요청 한도에 걸렸습니다. 잠시 후 다시 시도해 주세요.".into()),
        500..=599 => AppError::Ai("Gemini 서버에 일시적인 문제가 있습니다. 잠시 후 다시 시도해 주세요.".into()),
        _ => AppError::Ai(format!("Gemini 오류 ({status} {st}): {short}")),
    };
    (err, retry, status == 429 && !per_day)
}

fn net_err(e: reqwest::Error) -> AppError {
    if e.is_timeout() {
        AppError::Ai("Gemini 응답 시간이 초과되었습니다.".into())
    } else {
        AppError::Ai("Gemini 서버에 연결할 수 없습니다. 인터넷 연결을 확인해 주세요.".into())
    }
}

impl Gemini {
    pub fn new(key: &str) -> AppResult<Self> {
        Self::with_base(API_HOST, key)
    }

    /// 시험용: 루프백 주소로만 바꿀 수 있다.
    pub fn with_base(base: &str, key: &str) -> AppResult<Self> {
        let ok = base == API_HOST || base.starts_with("http://127.0.0.1:") || base.starts_with("http://localhost:");
        if !ok {
            return Err(AppError::Blocked("허용되지 않은 Gemini 주소입니다".into()));
        }
        let key = validate_key_format(key)?;
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|_| AppError::Ai("HTTP 클라이언트를 만들지 못했습니다".into()))?;
        Ok(Self { base: base.trim_end_matches('/').to_string(), key, http })
    }

    /// 키 확인: 사용 가능한 모델 목록 (이 앱에서 고를 수 있는 모델만)
    pub async fn verify(&self) -> AppResult<Vec<String>> {
        let mut names = Vec::new();
        let mut page: Option<String> = None;
        for _ in 0..10 {
            let mut url = format!("{}/v1beta/models?pageSize=200", self.base);
            if let Some(p) = &page {
                url.push_str(&format!("&pageToken={p}"));
            }
            let r = self.http.get(url).header("x-goog-api-key", &self.key).send().await.map_err(net_err)?;
            let status = r.status().as_u16();
            let v: Value = r.json().await.unwrap_or(Value::Null);
            if status >= 400 {
                return Err(translate_error(status, &v).0);
            }
            for m in v["models"].as_array().cloned().unwrap_or_default() {
                if let Some(n) = m["name"].as_str() {
                    names.push(n.trim_start_matches("models/").to_string());
                }
            }
            page = v["nextPageToken"].as_str().map(String::from);
            if page.is_none() {
                break;
            }
        }
        Ok(names)
    }

    /// 생성 (스트리밍). on_wait: 분당 한도 때문에 기다릴 때 남은 초를 알린다.
    #[allow(clippy::too_many_arguments)]
    pub async fn chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        temperature: f32,
        json_format: bool,
        rpm: u32,
        cancel: Arc<AtomicBool>,
        mut on_token: impl FnMut(&str) + Send,
        mut on_request: impl FnMut() + Send,
        mut on_wait: impl FnMut(u64) + Send,
    ) -> AppResult<(String, Usage)> {
        if !MODELS.iter().any(|m| m.0 == model) {
            return Err(AppError::Invalid(format!("지원하지 않는 Gemini 모델입니다: {model}")));
        }
        let system: Vec<&str> = messages.iter().filter(|m| m.role == "system").map(|m| m.content.as_str()).collect();
        let contents: Vec<Value> = messages
            .iter()
            .filter(|m| m.role != "system")
            .map(|m| json!({"role": if m.role == "assistant" { "model" } else { "user" }, "parts": [{"text": m.content}]}))
            .collect();
        let mut gen = json!({"temperature": temperature});
        if json_format {
            gen["responseMimeType"] = json!("application/json");
        }
        let mut body = json!({"contents": contents, "generationConfig": gen});
        if !system.is_empty() {
            body["systemInstruction"] = json!({"parts": [{"text": system.join("\n\n")}]});
        }
        let url = format!("{}/v1beta/models/{}:streamGenerateContent?alt=sse", self.base, model);
        let mut retried = false;
        loop {
            // 분당 한도 조절
            while let Some(w) = reserve_slot(model, rpm, Instant::now()) {
                on_wait(w.as_secs().max(1));
                let until = Instant::now() + w;
                while Instant::now() < until {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(AppError::Cancelled);
                    }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            }
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::Cancelled);
            }
            on_request();
            let r = self.http.post(&url).header("x-goog-api-key", &self.key).json(&body).send().await.map_err(net_err)?;
            let status = r.status().as_u16();
            if status >= 400 {
                let v: Value = r.json().await.unwrap_or(Value::Null);
                let (err, retry, per_minute) = translate_error(status, &v);
                if per_minute && !retried {
                    retried = true;
                    let w = retry.unwrap_or(Duration::from_secs(20)).min(Duration::from_secs(65));
                    on_wait(w.as_secs().max(1));
                    let until = Instant::now() + w;
                    while Instant::now() < until {
                        if cancel.load(Ordering::Relaxed) {
                            return Err(AppError::Cancelled);
                        }
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                    continue;
                }
                return Err(err);
            }
            return self.read_stream(r, cancel, &mut on_token).await;
        }
    }

    async fn read_stream(&self, r: reqwest::Response, cancel: Arc<AtomicBool>, on_token: &mut (impl FnMut(&str) + Send)) -> AppResult<(String, Usage)> {
        let mut stream = r.bytes_stream();
        let mut buf = String::new();
        let mut out = String::new();
        let mut usage = Usage::default();
        let mut blocked: Option<String> = None;
        let mut handle = |data: &str, out: &mut String, usage: &mut Usage, blocked: &mut Option<String>| {
            let Ok(v) = serde_json::from_str::<Value>(data) else { return };
            if let Some(b) = v["promptFeedback"]["blockReason"].as_str() {
                *blocked = Some(b.to_string());
            }
            if let Some(parts) = v["candidates"][0]["content"]["parts"].as_array() {
                for p in parts {
                    // 생각(thought) 부분은 답변에 넣지 않는다
                    if p["thought"].as_bool() == Some(true) {
                        continue;
                    }
                    if let Some(t) = p["text"].as_str() {
                        if !t.is_empty() {
                            out.push_str(t);
                            on_token(t);
                        }
                    }
                }
            }
            if matches!(v["candidates"][0]["finishReason"].as_str(), Some("SAFETY") | Some("PROHIBITED_CONTENT") | Some("BLOCKLIST")) {
                *blocked = Some(v["candidates"][0]["finishReason"].as_str().unwrap_or("").to_string());
            }
            if let Some(u) = v.get("usageMetadata") {
                usage.input_tokens = u["promptTokenCount"].as_u64().unwrap_or(usage.input_tokens);
                usage.output_tokens = u["candidatesTokenCount"].as_u64().unwrap_or(0) + u["thoughtsTokenCount"].as_u64().unwrap_or(0);
            }
        };
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::Cancelled);
            }
            let chunk = chunk.map_err(|_| AppError::Ai("Gemini와의 연결이 끊겼습니다".into()))?;
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(p) = buf.find('\n') {
                let line: String = buf.drain(..=p).collect();
                let line = line.trim();
                if let Some(d) = line.strip_prefix("data:") {
                    handle(d.trim(), &mut out, &mut usage, &mut blocked);
                }
            }
        }
        if let Some(d) = buf.trim().strip_prefix("data:") {
            handle(d.trim(), &mut out, &mut usage, &mut blocked);
        }
        if out.trim().is_empty() {
            if let Some(b) = blocked {
                return Err(AppError::Ai(format!("Gemini가 안전 정책으로 응답을 거부했습니다 ({b}).")));
            }
        }
        Ok((out, usage))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_google_or_loopback() {
        let k = "AIzaSyTESTKEY_1234567890abcd";
        assert!(Gemini::new(k).is_ok());
        assert!(Gemini::with_base("http://127.0.0.1:9999", k).is_ok());
        assert!(Gemini::with_base("https://evil.example.com", k).is_err());
        assert!(Gemini::new("short").is_err());
        assert!(Gemini::new("has space in key 123456789").is_err());
    }

    #[test]
    fn masking() {
        assert_eq!(mask_key("AIzaSyTESTKEY_1234567890abcd"), "AIza…abcd");
        assert!(!mask_key("AIzaSyTESTKEY_1234567890abcd").contains("TESTKEY"));
    }

    #[test]
    fn rpm_throttle() {
        let m = "test-throttle-model";
        let t0 = Instant::now();
        assert!(reserve_slot(m, 2, t0).is_none());
        assert!(reserve_slot(m, 2, t0).is_none());
        let w = reserve_slot(m, 2, t0 + Duration::from_secs(10)).unwrap();
        assert!(w > Duration::from_secs(49) && w < Duration::from_secs(51));
        assert!(reserve_slot(m, 2, t0 + Duration::from_secs(61)).is_none());
        assert!(reserve_slot(m, 0, t0).is_none());
    }

    #[test]
    fn error_translation() {
        let daily = json!({"error": {"code": 429, "status": "RESOURCE_EXHAUSTED", "message": "quota", "details": [
            {"@type": "type.googleapis.com/google.rpc.QuotaFailure", "violations": [{"quotaId": "GenerateRequestsPerDayPerProjectPerModel-FreeTier"}]},
            {"@type": "type.googleapis.com/google.rpc.RetryInfo", "retryDelay": "37s"}]}});
        let (e, _, retry) = translate_error(429, &daily);
        assert!(e.to_string().contains("오늘의 Gemini 무료 사용량"));
        assert!(!retry);
        let minute = json!({"error": {"code": 429, "message": "quota", "details": [
            {"violations": [{"quotaId": "GenerateRequestsPerMinutePerProjectPerModel-FreeTier"}]}, {"retryDelay": "12s"}]}});
        let (_, d, retry) = translate_error(429, &minute);
        assert!(retry);
        assert_eq!(d, Some(Duration::from_secs(12)));
        let bad = json!({"error": {"code": 400, "message": "API key not valid. Please pass a valid API key.", "details": [{"reason": "API_KEY_INVALID"}]}});
        assert!(translate_error(400, &bad).0.to_string().contains("키가 올바르지 않습니다"));
    }
}
