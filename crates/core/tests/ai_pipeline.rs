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
    let ai = rag::AiConfig { ollama: &ol, chat_model: Some("fake-chat:1b".into()), embed_model: Some("fake-embed".into()), index: Some(&idx) };
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
    let ai_dead = rag::AiConfig { ollama: &dead, chat_model: Some("x".into()), embed_model: None, index: None };
    let r3 = rag::ask(&db, &ai_dead, &rag::AskRequest { question: "비판적 사고 향상".into(), mode: search::Mode::Hybrid, paper_ids: vec![], top_k: 5 }, Arc::new(AtomicBool::new(false)), |_| {}).await.unwrap();
    assert_eq!(r3.status, "no_ai");
    assert!(!r3.other_sources.is_empty());
    assert_eq!(rag::recent_searches(&db.lock().unwrap(), 10).unwrap().len(), 2);

    // 논문 비교
    let cmp = compare::run(&db, &ol, "fake-chat:1b", &[ia.clone(), ib.clone()], &["purpose".into(), "sample_size".into(), "findings".into()], "", Arc::new(AtomicBool::new(false)), |_, _, _| {}).await.unwrap();
    assert_eq!(cmp.columns.len(), 2);
    let col = &cmp.columns[0];
    assert_eq!(col.cells["purpose"].status, "found");
    assert_eq!(col.cells["sample_size"].status, "not_found", "가짜 근거 번호는 인정하지 않음");
    let out = d.path().join("cmp.xlsx");
    export::export_comparison(&cmp, export::Format::Xlsx, &out).unwrap();
    assert!(out.exists());

    // 문헌고찰
    let rv = review::run(&db, &ol, "fake-chat:1b", &[ia.clone(), ib.clone()], "생성형 AI와 비판적 사고", Arc::new(AtomicBool::new(false)), |_| {}).await.unwrap();
    assert_eq!(rv.references.len(), 2);
    assert!(rv.notice.contains("검토용 초안"));
    assert!(!rv.draft.contains("S77"));
    let out = d.path().join("review.docx");
    export::export_review(&rv, export::Format::Docx, &out).unwrap();
    assert!(std::fs::metadata(&out).unwrap().len() > 1000);
}
