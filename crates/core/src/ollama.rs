//! 로컬 Ollama 클라이언트.
//!
//! - 이 컴퓨터(루프백 주소)의 Ollama에만 연결한다. 다른 주소는 설정할 수 없다.
//! - 시스템 프록시를 사용하지 않는다 (로컬 요청이 외부 프록시로 새지 않도록).
//! - Ollama의 클라우드 모델(이름에 `cloud`가 들어간 모델, 원격 실행 모델)은 사용하지 않는다.
//!   클라우드 모델은 연구자료를 외부 서버로 보내기 때문이다.
use crate::error::{AppError, AppResult};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const DEFAULT_PORT: u16 = 11434;

#[derive(Clone)]
pub struct Ollama {
    base: url::Url,
    http: reqwest::Client,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Health {
    pub running: bool,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstalledModel {
    pub name: String,
    pub size_bytes: u64,
    pub parameter_size: Option<String>,
    pub quantization: Option<String>,
    pub family: Option<String>,
    pub is_remote: bool,
    pub is_embedding: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunningModel {
    pub name: String,
    pub size_bytes: u64,
    pub vram_bytes: u64,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct PullProgress {
    pub status: String,
    pub total: u64,
    pub completed: u64,
    pub done: bool,
}

pub fn is_cloud_model(name: &str) -> bool {
    let l = name.to_lowercase();
    l.ends_with("-cloud") || l.contains(":cloud") || l.contains("-cloud:") || l.ends_with(":cloud")
}

pub fn looks_like_embedding_model(name: &str, family: Option<&str>) -> bool {
    let l = name.to_lowercase();
    let f = family.unwrap_or("").to_lowercase();
    ["embed", "bge", "e5", "minilm", "gte", "nomic-bert", "bert"].iter().any(|k| l.contains(k) || f.contains(k))
}

fn net_err(e: reqwest::Error) -> AppError {
    if e.is_connect() {
        AppError::Ai("로컬 AI(Ollama)에 연결할 수 없습니다. Ollama가 설치되어 실행 중인지 확인해 주세요.".into())
    } else if e.is_timeout() {
        AppError::Ai("로컬 AI가 응답하지 않습니다 (시간 초과). 더 가벼운 모델을 사용해 보세요.".into())
    } else {
        AppError::Ai("로컬 AI와 통신하지 못했습니다.".into())
    }
}

pub fn translate_error(status: u16, msg: &str) -> AppError {
    let m = msg.to_lowercase();
    if m.contains("requires more system memory") || m.contains("out of memory") || m.contains("insufficient memory") {
        AppError::Ai("이 AI 모델을 실행하기에 메모리가 부족합니다. 설정 > AI 모델에서 더 가벼운 모델을 선택하거나, 실행 중인 모델을 종료해 주세요.".into())
    } else if m.contains("not found") || status == 404 {
        AppError::Ai("선택한 AI 모델이 설치되어 있지 않습니다. 설정 > AI 모델에서 모델을 내려받아 주세요.".into())
    } else if m.contains("does not support embeddings") || m.contains("embedding") {
        AppError::Ai("선택한 모델은 임베딩(의미 검색)을 지원하지 않습니다. 임베딩 전용 모델(예: bge-m3)을 선택해 주세요.".into())
    } else if m.contains("no space left") {
        AppError::Ai("저장 공간이 부족합니다.".into())
    } else {
        let short: String = msg.chars().take(160).collect();
        AppError::Ai(format!("로컬 AI 오류 (코드 {status}): {short}"))
    }
}

impl Ollama {
    pub fn new(port: u16) -> AppResult<Self> {
        Self::with_host("127.0.0.1", port)
    }

    pub fn with_host(host: &str, port: u16) -> AppResult<Self> {
        if !matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]") {
            return Err(AppError::Blocked("로컬 AI는 이 컴퓨터(127.0.0.1)에서 실행되는 Ollama만 사용할 수 있습니다".into()));
        }
        let base = url::Url::parse(&format!("http://{host}:{port}/")).map_err(|_| AppError::Invalid("주소 형식 오류".into()))?;
        let http = reqwest::Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_secs(3))
            .build()
            .map_err(|_| AppError::Ai("HTTP 클라이언트를 만들지 못했습니다".into()))?;
        Ok(Self { base, http })
    }

    fn url(&self, p: &str) -> url::Url {
        self.base.join(p).unwrap()
    }

    async fn check(resp: reqwest::Response) -> AppResult<reqwest::Response> {
        if resp.status().is_success() {
            return Ok(resp);
        }
        let status = resp.status().as_u16();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        Err(translate_error(status, v["error"].as_str().unwrap_or("")))
    }

    async fn get(&self, p: &str) -> AppResult<Value> {
        let r = self.http.get(self.url(p)).timeout(Duration::from_secs(8)).send().await.map_err(net_err)?;
        Self::check(r).await?.json().await.map_err(net_err)
    }

    pub async fn health(&self) -> Health {
        match self.get("api/version").await {
            Ok(v) => Health { running: true, version: v["version"].as_str().map(String::from) },
            Err(_) => Health::default(),
        }
    }

    pub async fn list_models(&self) -> AppResult<Vec<InstalledModel>> {
        let v = self.get("api/tags").await?;
        let mut out = Vec::new();
        for m in v["models"].as_array().cloned().unwrap_or_default() {
            let Some(name) = m["name"].as_str().or(m["model"].as_str()).map(String::from) else { continue };
            let family = m["details"]["family"].as_str().map(String::from);
            let is_remote = m["remote_host"].as_str().map(|s| !s.is_empty()).unwrap_or(false) || is_cloud_model(&name);
            out.push(InstalledModel {
                is_embedding: looks_like_embedding_model(&name, family.as_deref()),
                size_bytes: m["size"].as_u64().unwrap_or(0),
                parameter_size: m["details"]["parameter_size"].as_str().map(String::from),
                quantization: m["details"]["quantization_level"].as_str().map(String::from),
                family,
                is_remote,
                name,
            });
        }
        Ok(out)
    }

    pub async fn running(&self) -> AppResult<Vec<RunningModel>> {
        let v = self.get("api/ps").await?;
        Ok(v["models"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|m| RunningModel {
                name: m["name"].as_str().unwrap_or("").into(),
                size_bytes: m["size"].as_u64().unwrap_or(0),
                vram_bytes: m["size_vram"].as_u64().unwrap_or(0),
                expires_at: m["expires_at"].as_str().unwrap_or("").into(),
            })
            .collect())
    }

    /// 메모리에 올라가 있는 모델을 내린다 (keep_alive = 0).
    pub async fn unload(&self, model: &str) -> AppResult<()> {
        let embed = looks_like_embedding_model(model, None);
        let (path, body) = if embed {
            ("api/embed", json!({"model": model, "input": [], "keep_alive": 0}))
        } else {
            ("api/generate", json!({"model": model, "keep_alive": 0}))
        };
        let r = self.http.post(self.url(path)).json(&body).timeout(Duration::from_secs(30)).send().await.map_err(net_err)?;
        Self::check(r).await?;
        Ok(())
    }

    pub async fn delete_model(&self, model: &str) -> AppResult<()> {
        let r = self.http.delete(self.url("api/delete")).json(&json!({"model": model})).timeout(Duration::from_secs(30)).send().await.map_err(net_err)?;
        Self::check(r).await?;
        Ok(())
    }

    /// 모델 내려받기 (Ollama가 공식 저장소에서 받음). 진행률을 콜백으로 알린다.
    pub async fn pull(&self, model: &str, cancel: Arc<AtomicBool>, mut on: impl FnMut(PullProgress) + Send) -> AppResult<()> {
        if is_cloud_model(model) {
            return Err(AppError::Blocked("클라우드 모델은 연구자료를 외부로 보내므로 사용할 수 없습니다".into()));
        }
        let r = self.http.post(self.url("api/pull")).json(&json!({"model": model, "stream": true})).send().await.map_err(net_err)?;
        let r = Self::check(r).await?;
        let mut layers: std::collections::HashMap<String, (u64, u64)> = Default::default();
        let mut ok = false;
        let mut lines = Lines::new(r);
        while let Some(line) = lines.next().await? {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::Cancelled);
            }
            let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
            if let Some(e) = v["error"].as_str() {
                return Err(translate_error(500, e));
            }
            if let (Some(d), Some(t)) = (v["digest"].as_str(), v["total"].as_u64()) {
                layers.insert(d.into(), (t, v["completed"].as_u64().unwrap_or(0)));
            }
            let status = v["status"].as_str().unwrap_or("").to_string();
            ok |= status == "success";
            let (total, completed) = layers.values().fold((0, 0), |a, l| (a.0 + l.0, a.1 + l.1));
            on(PullProgress { status, total, completed, done: ok });
        }
        if ok { Ok(()) } else { Err(AppError::Ai("내려받기가 중간에 끊겼습니다. 다시 시도하면 이어서 받습니다.".into())) }
    }

    /// 대화 생성 (스트리밍). 토큰마다 콜백. 취소 플래그가 서면 중단한다.
    pub async fn chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        temperature: f32,
        num_ctx: Option<u32>,
        json_format: bool,
        cancel: Arc<AtomicBool>,
        mut on_token: impl FnMut(&str) + Send,
    ) -> AppResult<String> {
        if is_cloud_model(model) {
            return Err(AppError::Blocked("클라우드 모델은 연구자료를 외부로 보내므로 사용할 수 없습니다".into()));
        }
        let mut options = json!({"temperature": temperature});
        if let Some(n) = num_ctx {
            options["num_ctx"] = json!(n);
        }
        let mut body = json!({"model": model, "messages": messages, "stream": true, "think": false, "options": options});
        if json_format {
            body["format"] = json!("json");
        }
        let r = self.http.post(self.url("api/chat")).json(&body).send().await.map_err(net_err)?;
        let r = match Self::check(r).await {
            Err(AppError::Ai(m)) if m.contains("코드 400") && m.contains("think") => {
                // think 옵션을 모르는 모델 → 빼고 다시 요청
                body.as_object_mut().unwrap().remove("think");
                let r = self.http.post(self.url("api/chat")).json(&body).send().await.map_err(net_err)?;
                Self::check(r).await?
            }
            other => other?,
        };
        let mut out = String::new();
        let mut lines = Lines::new(r);
        while let Some(line) = lines.next().await? {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::Cancelled);
            }
            let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
            if let Some(e) = v["error"].as_str() {
                return Err(translate_error(500, e));
            }
            if v["remote_host"].as_str().map(|s| !s.is_empty()).unwrap_or(false) {
                return Err(AppError::Blocked("원격에서 실행된 응답이 감지되어 중단했습니다".into()));
            }
            if let Some(t) = v["message"]["content"].as_str() {
                if !t.is_empty() {
                    out.push_str(t);
                    on_token(t);
                }
            }
            if v["done"].as_bool() == Some(true) {
                break;
            }
        }
        Ok(out)
    }

    /// 임베딩 벡터 생성
    pub async fn embed(&self, model: &str, inputs: &[String]) -> AppResult<Vec<Vec<f32>>> {
        if is_cloud_model(model) {
            return Err(AppError::Blocked("클라우드 모델은 사용할 수 없습니다".into()));
        }
        let r = self
            .http
            .post(self.url("api/embed"))
            .json(&json!({"model": model, "input": inputs, "truncate": true}))
            .timeout(Duration::from_secs(600))
            .send()
            .await
            .map_err(net_err)?;
        let v: Value = Self::check(r).await?.json().await.map_err(net_err)?;
        let arr = v["embeddings"].as_array().ok_or_else(|| AppError::Ai("임베딩 응답 형식이 올바르지 않습니다".into()))?;
        let out: Vec<Vec<f32>> = arr.iter().map(|e| e.as_array().map(|x| x.iter().map(|f| f.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_default()).collect();
        if out.len() != inputs.len() || out.iter().any(|v| v.is_empty()) {
            return Err(AppError::Ai("임베딩 결과 개수가 맞지 않습니다".into()));
        }
        Ok(out)
    }
}

struct Lines {
    stream: futures_util::stream::BoxStream<'static, Result<Vec<u8>, reqwest::Error>>,
    buf: Vec<u8>,
    ended: bool,
}

impl Lines {
    fn new(r: reqwest::Response) -> Self {
        Self { stream: r.bytes_stream().map(|b| b.map(|x| x.to_vec())).boxed(), buf: vec![], ended: false }
    }
    async fn next(&mut self) -> AppResult<Option<String>> {
        loop {
            if let Some(p) = self.buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = self.buf.drain(..=p).collect();
                let s = String::from_utf8_lossy(&line).trim().to_string();
                if s.is_empty() {
                    continue;
                }
                return Ok(Some(s));
            }
            if self.ended {
                let s = String::from_utf8_lossy(&std::mem::take(&mut self.buf)).trim().to_string();
                return Ok(if s.is_empty() { None } else { Some(s) });
            }
            match self.stream.next().await {
                Some(Ok(c)) => self.buf.extend_from_slice(&c),
                Some(Err(_)) => return Err(AppError::Ai("로컬 AI와의 연결이 끊겼습니다".into())),
                None => self.ended = true,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_loopback_allowed() {
        assert!(Ollama::with_host("127.0.0.1", 11434).is_ok());
        assert!(Ollama::with_host("localhost", 11434).is_ok());
        assert!(Ollama::with_host("example.com", 11434).is_err());
        assert!(Ollama::with_host("10.0.0.5", 11434).is_err());
    }
    #[test]
    fn cloud_models_detected() {
        assert!(is_cloud_model("gpt-oss:120b-cloud"));
        assert!(is_cloud_model("qwen3-coder:480b-cloud"));
        assert!(!is_cloud_model("qwen3:8b"));
    }
    #[test]
    fn embedding_detection() {
        assert!(looks_like_embedding_model("bge-m3:latest", None));
        assert!(looks_like_embedding_model("nomic-embed-text", None));
        assert!(!looks_like_embedding_model("gemma3:4b", Some("gemma3")));
    }
}
