//! 가짜 Ollama 서버(127.0.0.1)로 AI 검색·인용 검증·비교·문헌고찰 전체 흐름을 검증한다.
use prh_core::*;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn embed_text(t: &str) -> Vec<f32> {
    let mut v = vec![0f32; 64];
    for tok in text::tokens(t) {
        let h = tok.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
        v[(h % 64) as usize] += 1.0;
    }
    v
}

async fn handle(mut s: tokio::net::TcpStream) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    let (head_end, len) = loop {
        let n = s.read(&mut tmp).await.unwrap();
        if n == 0 {
            return;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..p]).to_lowercase();
            let len = head.lines().find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap())).unwrap_or(0);
            break (p + 4, len);
        }
    };
    while buf.len() < head_end + len {
        let n = s.read(&mut tmp).await.unwrap();
        buf.extend_from_slice(&tmp[..n]);
    }
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let body: serde_json::Value = serde_json::from_slice(&buf[head_end..head_end + len]).unwrap_or(serde_json::Value::Null);
    let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
    if path.starts_with("/v1beta/") {
        return gemini_handle(s, &head, &path, body).await;
    }
    let out = match path.as_str() {
        "/api/version" => r#"{"version":"0.0-fake"}"#.to_string(),
        "/api/tags" => r#"{"models":[{"name":"fake-chat:1b","size":1,"details":{"family":"gemma3"}},{"name":"fake-embed","size":1,"details":{"family":"bert"}}]}"#.into(),
        "/api/embed" => {
            let inputs: Vec<String> = body["input"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect();
            serde_json::json!({"embeddings": inputs.iter().map(|i| embed_text(i)).collect::<Vec<_>>()}).to_string()
        }
        "/api/chat" => {
            let user = body["messages"][1]["content"].as_str().unwrap_or("");
            let reply = if body["format"] == "json" {
                // 첫 번째 출처의 첫 문장을 목적·결과로 사용, 표본은 가짜 번호
                let first = user.split("<source id=\"S1\"").nth(1).and_then(|x| x.split('\n').nth(1)).unwrap_or("").to_string();
                serde_json::json!({"purpose": {"value": first, "sources": ["S1"]}, "sample_size": {"value": "999명", "sources": ["S42"]}, "findings": {"value": first, "sources": ["S1"]}}).to_string()
            } else if user.contains("양자 중력") {
                "[근거 부족]".to_string()
            } else {
                let first = user.split("<source id=\"S1\"").nth(1).and_then(|x| x.split('\n').nth(1)).unwrap_or("").to_string();
                format!("{first} [S1]. 이것은 지어낸 연구입니다 [S77]. (Park, 2019)도 그렇다고 했습니다.")
            };
            let mut lines = String::new();
            for ch in reply.chars().collect::<Vec<_>>().chunks(7) {
                lines.push_str(&serde_json::json!({"message": {"role":"assistant","content": ch.iter().collect::<String>()}, "done": false}).to_string());
                lines.push('\n');
            }
            lines.push_str("{\"done\":true,\"eval_count\":10,\"eval_duration\":1000000}\n");
            lines
        }
        _ => r#"{"error":"not found"}"#.into(),
    };
    let status = if out.contains("not found") { "404 Not Found" } else { "200 OK" };
    let resp = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}", out.len());
    let _ = s.write_all(resp.as_bytes()).await;
}

/// 가짜 Gemini: 키 "AIzaFAKE_DAILY_EXHAUSTED_0000"이면 하루 한도 초과(429)를 돌려준다
async fn gemini_handle(mut s: tokio::net::TcpStream, head: &str, path: &str, body: serde_json::Value) {
    let key = head.lines().find_map(|l| l.to_lowercase().strip_prefix("x-goog-api-key:").map(|v| v.trim().to_string())).unwrap_or_default();
    let (status, ctype, out) = if key.is_empty() || key.contains("bad") {
        ("400 Bad Request", "application/json", r#"{"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","details":[{"reason":"API_KEY_INVALID"}]}}"#.to_string())
    } else if key.contains("daily_exhausted") {
        ("429 Too Many Requests", "application/json", r#"{"error":{"code":429,"status":"RESOURCE_EXHAUSTED","message":"quota","details":[{"violations":[{"quotaId":"GenerateRequestsPerDayPerProjectPerModel-FreeTier"}]}]}}"#.to_string())
    } else if path.starts_with("/v1beta/models?") {
        ("200 OK", "application/json", r#"{"models":[{"name":"models/gemini-3.8-flash"},{"name":"models/gemini-3.5-flash-lite"},{"name":"models/other"}]}"#.to_string())
    } else {
        assert!(body["systemInstruction"]["parts"][0]["text"].as_str().unwrap().contains("[S1]"), "시스템 지침 전달");
        let user = body["contents"][0]["parts"][0]["text"].as_str().unwrap_or("");
        let first = user.split("<source id=\"S1\"").nth(1).and_then(|x| x.split('\n').nth(1)).unwrap_or("").to_string();
        let reply = format!("{first} [S1]. 가짜 [S5].");
        let mid = reply.chars().count() / 2;
        let (a, b): (String, String) = (reply.chars().take(mid).collect(), reply.chars().skip(mid).collect());
        let mut sse = String::new();
        sse.push_str(&format!("data: {}\r\n\r\n", serde_json::json!({"candidates":[{"content":{"parts":[{"text":"생각 중","thought":true},{"text":a}]}}]})));
        sse.push_str(&format!("data: {}\r\n\r\n", serde_json::json!({"candidates":[{"content":{"parts":[{"text":b}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":321,"candidatesTokenCount":12,"thoughtsTokenCount":3}})));
        ("200 OK", "text/event-stream", sse)
    };
    let resp = format!("HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}", out.len());
    let _ = s.write_all(resp.as_bytes()).await;
}

async fn fake_ollama() -> u16 {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let (s, _) = l.accept().await.unwrap();
            tokio::spawn(handle(s));
        }
    });
    port
}

fn write_txt(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, body).unwrap();
    p
}

#[tokio::test(flavor = "multi_thread")]
async fn full_ai_flow() {
    let port = fake_ollama().await;
    let ol = ollama::Ollama::new(port).unwrap();
    assert!(ol.health().await.running);
    assert_eq!(ol.list_models().await.unwrap().iter().filter(|m| m.is_embedding).count(), 1);

    let d = tempfile::tempdir().unwrap();
    let paths = paths::LibraryPaths::new(d.path().join("lib"));
    paths.ensure(false).unwrap();
    let (mut c, _) = db::open(&paths).unwrap();
    let a = write_txt(d.path(), "a.txt", "생성형 AI를 활용한 글쓰기 수업에서 대학생의 비판적 사고 점수가 유의하게 향상되었다.\n\n연구 대상은 대학생 120명이었다.");
    let b = write_txt(d.path(), "b.md", "# 토론 수업\n\n토론 중심 수업은 학생들의 논증 능력을 높였다. 생성형 AI는 사용하지 않았다.");
    let ia = library::import_file(&mut c, &paths, &a).unwrap().paper_id.unwrap();
    let ib = library::import_file(&mut c, &paths, &b).unwrap().paper_id.unwrap();

    // 임베딩 색인
    let pend = search::pending_chunks(&c, "fake-embed", 100).unwrap();
    let inputs: Vec<String> = pend.iter().map(|(_, t, x)| search::embed_input(t, x)).collect();
    let vecs = ol.embed("fake-embed", &inputs).await.unwrap();
    let items: Vec<(i64, Vec<f32>)> = pend.iter().map(|p| p.0).zip(vecs).collect();
    search::store_embeddings(&mut c, "fake-embed", &items).unwrap();
    let idx = search::load_index(&c, "fake-embed").unwrap();
    assert!(!idx.ids.is_empty());

    let db = Mutex::new(c);
    let chat = llm::Llm::ollama(ol.clone(), "fake-chat:1b");
    let ai = rag::AiConfig { llm: Some(&chat), ollama: &ol, embed_model: Some("fake-embed".into()), index: Some(&idx) };
    let mut streamed = String::new();
    let r = rag::ask(&db, &ai, &rag::AskRequest { question: "생성형 AI와 비판적 사고의 관계를 다룬 연구를 찾아줘".into(), mode: search::Mode::Hybrid, paper_ids: vec![], top_k: 5 }, Arc::new(AtomicBool::new(false)), |t| streamed.push_str(t)).await.unwrap();
    assert_eq!(r.status, "answered");
    assert_eq!(r.mode_used, search::Mode::Hybrid);
    assert!(streamed.contains("[S77]"), "스트리밍 원문에는 있었지만");
    assert!(!r.answer.contains("S77"), "검증 후 존재하지 않는 출처는 제거");
    assert_eq!(r.removed_markers, 1);
    assert_eq!(r.removed_references, vec!["(Park, 2019)"]);
    assert_eq!(r.citations.len(), 1);
    assert_eq!(r.citations[0].source.paper_id, ia);
    assert_eq!(r.citations[0].support, "verified");

    // 근거 없는 질문 → AI 호출 없이 안내
    let r2 = rag::ask(&db, &ai, &rag::AskRequest { question: "양자 중력 이론의 최신 실험".into(), mode: search::Mode::Keyword, paper_ids: vec![], top_k: 5 }, Arc::new(AtomicBool::new(false)), |_| {}).await.unwrap();
    assert_eq!(r2.status, "insufficient", "{:?}", r2.other_sources.iter().map(|s| &s.text).collect::<Vec<_>>());
    assert_eq!(r2.answer, rag::INSUFFICIENT);

    // Ollama 중단 상태 → 검색 결과만
    let dead = ollama::Ollama::new(1).unwrap();
    let dead_llm = llm::Llm::ollama(dead.clone(), "x");
    let ai_dead = rag::AiConfig { llm: Some(&dead_llm), ollama: &dead, embed_model: None, index: None };
    let r3 = rag::ask(&db, &ai_dead, &rag::AskRequest { question: "비판적 사고 향상".into(), mode: search::Mode::Hybrid, paper_ids: vec![], top_k: 5 }, Arc::new(AtomicBool::new(false)), |_| {}).await.unwrap();
    assert_eq!(r3.status, "no_ai");
    assert!(!r3.other_sources.is_empty());
    assert_eq!(rag::recent_searches(&db.lock().unwrap(), 10).unwrap().len(), 2);

    // 논문 비교
    let cmp = compare::run(&db, &chat, &[ia.clone(), ib.clone()], &["purpose".into(), "sample_size".into(), "findings".into()], "", Arc::new(AtomicBool::new(false)), |_, _, _| {}).await.unwrap();
    assert_eq!(cmp.columns.len(), 2);
    let col = &cmp.columns[0];
    assert_eq!(col.cells["purpose"].status, "found");
    assert_eq!(col.cells["sample_size"].status, "not_found", "가짜 근거 번호는 인정하지 않음");
    let out = d.path().join("cmp.xlsx");
    export::export_comparison(&cmp, export::Format::Xlsx, &out).unwrap();
    assert!(out.exists());

    // 문헌고찰
    let rv = review::run(&db, &chat, &[ia.clone(), ib.clone()], "생성형 AI와 비판적 사고", Arc::new(AtomicBool::new(false)), |_| {}).await.unwrap();
    assert_eq!(rv.references.len(), 2);
    assert!(rv.notice.contains("검토용 초안"));
    assert!(!rv.draft.contains("S77"));
    let out = d.path().join("review.docx");
    export::export_review(&rv, export::Format::Docx, &out).unwrap();
    assert!(std::fs::metadata(&out).unwrap().len() > 1000);
}

#[tokio::test(flavor = "multi_thread")]
async fn gemini_flow_records_usage_and_verifies_citations() {
    let port = fake_ollama().await;
    let base = format!("http://127.0.0.1:{port}");
    let d = tempfile::tempdir().unwrap();
    let paths = paths::LibraryPaths::new(d.path().join("lib"));
    paths.ensure(false).unwrap();
    let (mut c, _) = db::open(&paths).unwrap();
    let a = write_txt(d.path(), "a.txt", "생성형 AI를 활용한 글쓰기 수업에서 대학생의 비판적 사고 점수가 유의하게 향상되었다.");
    library::import_file(&mut c, &paths, &a).unwrap();
    let db = std::sync::Arc::new(Mutex::new(c));

    // 키 확인
    let g = gemini::Gemini::with_base(&base, "AIzaFAKE_GOOD_KEY_000000000").unwrap();
    let models = g.verify().await.unwrap();
    assert!(models.contains(&"gemini-3.8-flash".to_string()));
    let bad = gemini::Gemini::with_base(&base, "AIzaFAKE_bad_KEY_0000000000").unwrap();
    assert!(bad.verify().await.unwrap_err().to_string().contains("키가 올바르지 않습니다"));

    // 사용량 기록 싱크
    let mut chat = llm::Llm::gemini(g, "gemini-3.8-flash", 0);
    let db2 = db.clone();
    chat.usage = Some(std::sync::Arc::new(move |ev| { usage::record(&db2.lock().unwrap(), ev, chrono::Utc::now()).unwrap(); }));
    let dead = ollama::Ollama::new(1).unwrap();
    let ai = rag::AiConfig { llm: Some(&chat), ollama: &dead, embed_model: None, index: None };
    let r = rag::ask(&db, &ai, &rag::AskRequest { question: "비판적 사고 향상".into(), mode: search::Mode::Keyword, paper_ids: vec![], top_k: 5 }, Arc::new(AtomicBool::new(false)), |_| {}).await.unwrap();
    assert_eq!(r.status, "answered");
    assert_eq!(r.model, "gemini-3.8-flash (Gemini API)");
    assert!(!r.answer.contains("생각 중"), "생각(thought) 부분 제외");
    assert_eq!(r.removed_markers, 1);
    assert_eq!(r.citations.len(), 1);
    let t = usage::today(&db.lock().unwrap(), &["gemini-3.8-flash"], &Default::default(), chrono::Utc::now()).unwrap();
    assert_eq!(t.models[0].requests, 1);
    assert_eq!(t.models[0].input_tokens, 321);
    assert_eq!(t.models[0].output_tokens, 15);

    // 하루 한도 초과 → 안내 + 요청 1건·오류 1건 기록, 검색 원문은 그대로 보여 줌
    let g2 = gemini::Gemini::with_base(&base, "AIzaFAKE_daily_exhausted_0000").unwrap();
    let mut chat2 = llm::Llm::gemini(g2, "gemini-3.8-flash", 0);
    let db3 = db.clone();
    chat2.usage = Some(std::sync::Arc::new(move |ev| { usage::record(&db3.lock().unwrap(), ev, chrono::Utc::now()).unwrap(); }));
    let ai2 = rag::AiConfig { llm: Some(&chat2), ollama: &dead, embed_model: None, index: None };
    let r2 = rag::ask(&db, &ai2, &rag::AskRequest { question: "비판적 사고 향상".into(), mode: search::Mode::Keyword, paper_ids: vec![], top_k: 5 }, Arc::new(AtomicBool::new(false)), |_| {}).await.unwrap();
    assert_eq!(r2.status, "no_ai");
    assert!(r2.answer.contains("오늘의 Gemini 무료 사용량"), "{}", r2.answer);
    assert!(!r2.other_sources.is_empty());
    let t = usage::today(&db.lock().unwrap(), &["gemini-3.8-flash"], &Default::default(), chrono::Utc::now()).unwrap();
    assert_eq!((t.models[0].requests, t.models[0].errors), (2, 1));
}
