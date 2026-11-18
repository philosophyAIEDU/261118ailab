//! 논문 비교: 선택한 논문(2~10편)을 항목별로 비교하고, 각 칸마다 원문 근거(쪽 번호)를 연결한다.
//!
//! 논문마다 항목별 관련 문단을 검색해 로컬 AI에게 주고, JSON으로 항목 값과 근거 번호를 받는다.
//! 근거 번호가 실제 제공한 문단이 아니거나 비어 있으면 그 칸은 "원문에서 확인되지 않음"으로 둔다.
use crate::error::{AppError, AppResult};
use crate::ollama::{ChatMessage, Ollama};
use crate::rag::{self, Source};
use crate::search;
use crate::text;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

pub struct Aspect {
    pub key: &'static str,
    pub label: &'static str,
    pub query: &'static str,
}

pub const ASPECTS: &[Aspect] = &[
    Aspect { key: "purpose", label: "연구 목적", query: "연구 목적 목적은 본 연구는 aim purpose objective this study examines investigates" },
    Aspect { key: "questions", label: "연구 질문", query: "연구 문제 연구 질문 가설 research question hypothesis RQ" },
    Aspect { key: "theory", label: "이론적 배경", query: "이론적 배경 이론 틀 개념 framework theory theoretical literature" },
    Aspect { key: "method", label: "연구 방법", query: "연구 방법 연구 설계 실험 조사 설문 면담 method methodology design procedure" },
    Aspect { key: "participants", label: "연구 대상", query: "연구 대상 참여자 대학생 학생 participants sample subjects students recruited" },
    Aspect { key: "sample_size", label: "표본 크기", query: "명 표본 참여자 수 N= n= participants sample size total" },
    Aspect { key: "analysis", label: "분석 방법", query: "분석 방법 통계 t검정 회귀 분산분석 주제분석 analysis regression ANOVA thematic coding" },
    Aspect { key: "findings", label: "주요 결과", query: "연구 결과 결과 나타났다 유의 results findings significant showed revealed" },
    Aspect { key: "limitations", label: "연구 한계", query: "한계 제한점 limitation limitations constraint generalizability" },
    Aspect { key: "future", label: "후속 연구 제안", query: "후속 연구 향후 연구 제언 future research further study recommend" },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    /// found / weak / not_found
    pub status: String,
    pub value: String,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub chunk_id: i64,
    pub page: Option<i64>,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperColumn {
    pub paper_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub cells: std::collections::BTreeMap<String, Cell>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comparison {
    pub id: String,
    pub title: String,
    pub aspects: Vec<(String, String)>,
    pub columns: Vec<PaperColumn>,
    pub model: String,
    pub created_at: String,
}

pub fn aspect_list(keys: &[String]) -> Vec<&'static Aspect> {
    if keys.is_empty() {
        ASPECTS.iter().collect()
    } else {
        ASPECTS.iter().filter(|a| keys.iter().any(|k| k == a.key)).collect()
    }
}

/// 한 논문에서 비교 항목별 근거 문단을 모은다
pub fn gather(conn: &Connection, paper_id: &str, aspects: &[&Aspect], max: usize) -> AppResult<Vec<search::Hit>> {
    let ids = vec![paper_id.to_string()];
    let mut seen = std::collections::HashSet::new();
    let mut hits = Vec::new();
    // 앞부분(초록·서론)은 항상 포함
    let mut order: Vec<i64> = conn.prepare("SELECT id FROM chunks WHERE paper_id=?1 ORDER BY ord LIMIT 2")?.query_map([paper_id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    for id in &order {
        seen.insert(*id);
    }
    for a in aspects {
        let terms: Vec<String> = a.query.split_whitespace().map(String::from).collect();
        for (id, _) in search::keyword_ids(conn, &terms, 2, Some(&ids))? {
            if seen.insert(id) {
                hits.push(id);
            }
        }
    }
    order.extend(hits);
    order.truncate(max);
    let mut st = conn.prepare_cached("SELECT c.id, c.paper_id, p.title, p.authors, p.year, c.page, c.text FROM chunks c JOIN papers p ON p.id=c.paper_id WHERE c.id=?1")?;
    let mut out = Vec::new();
    for id in order {
        out.push(st.query_row([id], |r| {
            Ok(search::Hit {
                chunk_id: r.get(0)?,
                paper_id: r.get(1)?,
                paper_title: r.get(2)?,
                authors: serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or_default(),
                year: r.get(4)?,
                page: r.get(5)?,
                text: r.get(6)?,
                score: 0.0,
                keyword_rank: None,
                vector_rank: None,
                vector_sim: None,
            })
        })?);
    }
    Ok(out)
}

pub fn messages(title: &str, sources: &[Source], aspects: &[&Aspect]) -> Vec<ChatMessage> {
    let keys: Vec<String> = aspects.iter().map(|a| format!("\"{}\" ({})", a.key, a.label)).collect();
    vec![
        ChatMessage { role: "system".into(), content: rag::SYSTEM_PROMPT.into() },
        ChatMessage {
            role: "user".into(),
            content: format!(
                "논문 「{}」에서 발췌한 자료입니다.\n\n{}\n다음 항목을 정리해 JSON 객체로만 답하세요: {}.\n형식: {{\"항목키\": {{\"value\": \"한두 문장 요약\", \"sources\": [\"S1\"]}}}}\n자료에서 확인되지 않는 항목은 {{\"value\": \"\", \"sources\": []}} 로 두세요. 추측하지 마세요. 표본 크기는 원문에 적힌 숫자만 쓰세요.",
                title,
                rag::source_block(sources),
                keys.join(", ")
            ),
        },
    ]
}

/// AI 응답(JSON)을 검증해 칸을 만든다
pub fn parse_cells(raw: &str, sources: &[Source], aspects: &[&Aspect]) -> std::collections::BTreeMap<String, Cell> {
    let v: Value = serde_json::from_str(raw.trim()).or_else(|_| {
        let s = raw.find('{').unwrap_or(0);
        let e = raw.rfind('}').map(|e| e + 1).unwrap_or(raw.len());
        serde_json::from_str(&raw[s..e.max(s)])
    }).unwrap_or(Value::Null);
    let mut out = std::collections::BTreeMap::new();
    for a in aspects {
        let item = &v[a.key];
        let value = item["value"].as_str().map(str::trim).unwrap_or("").to_string();
        let labels: Vec<String> = item["sources"].as_array().map(|x| x.iter().filter_map(|s| s.as_str().map(String::from).or_else(|| s.as_u64().map(|n| format!("S{n}")))).collect()).unwrap_or_default();
        let ev: Vec<&Source> = labels.iter().filter_map(|l| sources.iter().find(|s| s.label.eq_ignore_ascii_case(l.trim()))).collect();
        let not_found = value.is_empty() || ev.is_empty() || value.contains("확인되지 않");
        let cell = if not_found {
            Cell { status: "not_found".into(), value: String::new(), evidence: vec![] }
        } else {
            let best = ev.iter().map(|s| text::overlap_ratio(&value, &s.text)).fold(0.0, f64::max);
            Cell {
                status: if best >= 0.25 { "found".into() } else { "weak".into() },
                value,
                evidence: ev.iter().map(|s| Evidence { chunk_id: s.chunk_id, page: s.page, quote: text::preview(&s.text, 240) }).collect(),
            }
        };
        out.insert(a.key.to_string(), cell);
    }
    out
}

pub async fn run(
    db: &Mutex<Connection>,
    ollama: &Ollama,
    model: &str,
    paper_ids: &[String],
    aspect_keys: &[String],
    title: &str,
    cancel: Arc<AtomicBool>,
    mut progress: impl FnMut(usize, usize, &str) + Send,
) -> AppResult<Comparison> {
    if paper_ids.len() < 2 || paper_ids.len() > 10 {
        return Err(AppError::Invalid("비교할 논문을 2~10편 선택해 주세요".into()));
    }
    let aspects = aspect_list(aspect_keys);
    let mut columns = Vec::new();
    for (i, pid) in paper_ids.iter().enumerate() {
        let (paper, hits) = {
            let conn = db.lock().map_err(|_| AppError::Db("DB 잠금 오류".into()))?;
            (crate::library::get_paper(&conn, pid)?, gather(&conn, pid, &aspects, 12)?)
        };
        progress(i, paper_ids.len(), &paper.title);
        let mut col = PaperColumn { paper_id: pid.clone(), title: paper.title.clone(), authors: paper.authors.clone(), year: paper.year, cells: Default::default(), error: None };
        if hits.is_empty() {
            col.error = Some("본문이 없는 항목입니다 (서지정보만 있거나 글자를 추출하지 못함)".into());
            for a in &aspects {
                col.cells.insert(a.key.into(), Cell { status: "not_found".into(), value: String::new(), evidence: vec![] });
            }
            columns.push(col);
            continue;
        }
        let sources = rag::to_sources(&hits);
        match ollama.chat(model, &messages(&paper.title, &sources, &aspects), 0.1, Some(8192), true, cancel.clone(), |_| {}).await {
            Ok(raw) => col.cells = parse_cells(&raw, &sources, &aspects),
            Err(AppError::Cancelled) => return Err(AppError::Cancelled),
            Err(e) => {
                col.error = Some(e.to_string());
                for a in &aspects {
                    col.cells.insert(a.key.into(), Cell { status: "not_found".into(), value: String::new(), evidence: vec![] });
                }
            }
        }
        columns.push(col);
    }
    progress(paper_ids.len(), paper_ids.len(), "");
    let c = Comparison {
        id: crate::new_id(),
        title: if title.trim().is_empty() { format!("논문 비교 ({}편)", paper_ids.len()) } else { title.trim().to_string() },
        aspects: aspects.iter().map(|a| (a.key.to_string(), a.label.to_string())).collect(),
        columns,
        model: model.to_string(),
        created_at: crate::now(),
    };
    let conn = db.lock().map_err(|_| AppError::Db("DB 잠금 오류".into()))?;
    save(&conn, &c)?;
    Ok(c)
}

pub fn save(conn: &Connection, c: &Comparison) -> AppResult<()> {
    let pids: Vec<&String> = c.columns.iter().map(|x| &x.paper_id).collect();
    conn.execute(
        "INSERT OR REPLACE INTO comparisons(id,title,paper_ids,aspects,result,model,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![c.id, c.title, serde_json::to_string(&pids)?, serde_json::to_string(&c.aspects)?, serde_json::to_string(c)?, c.model, c.created_at],
    )?;
    Ok(())
}

pub fn list(conn: &Connection) -> AppResult<Vec<Comparison>> {
    let mut st = conn.prepare("SELECT result FROM comparisons ORDER BY created_at DESC")?;
    let v = st.query_map([], |r| r.get::<_, String>(0))?.filter_map(|s| s.ok().and_then(|s| serde_json::from_str(&s).ok())).collect();
    Ok(v)
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Comparison> {
    let s: String = conn.query_row("SELECT result FROM comparisons WHERE id=?1", [id], |r| r.get(0)).map_err(|_| AppError::NotFound("비교 결과를 찾을 수 없습니다".into()))?;
    Ok(serde_json::from_str(&s)?)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM comparisons WHERE id=?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(n: usize, t: &str) -> Source {
        Source { label: format!("S{n}"), chunk_id: n as i64, paper_id: "p".into(), paper_title: "t".into(), authors: vec![], year: None, page: Some(n as i64), text: t.into(), suspicious: false }
    }
    #[test]
    fn cells_require_real_evidence() {
        let sources = vec![s(1, "본 연구는 대학생 240명을 대상으로 설문 조사를 실시하였다."), s(2, "연구의 한계로 단일 대학 표본을 들 수 있다.")];
        let aspects = aspect_list(&["participants".into(), "sample_size".into(), "limitations".into(), "future".into(), "method".into()]);
        let raw = r#"```json
{"participants": {"value": "대학생 240명", "sources": ["S1"]},
 "sample_size": {"value": "240명", "sources": ["S9"]},
 "limitations": {"value": "단일 대학 표본이라는 한계", "sources": ["S2"]},
 "future": {"value": "", "sources": []},
 "method": {"value": "실험 연구", "sources": [1]}}
```"#;
        let c = parse_cells(raw, &sources, &aspects);
        assert_eq!(c["participants"].status, "found");
        assert_eq!(c["participants"].evidence[0].page, Some(1));
        assert_eq!(c["sample_size"].status, "not_found", "존재하지 않는 근거 번호");
        assert_eq!(c["limitations"].status, "found");
        assert_eq!(c["future"].status, "not_found");
        assert_eq!(c["method"].status, "weak", "근거와 겹침이 약하면 확인 필요");
        let bad = parse_cells("not json", &sources, &aspects);
        assert!(bad.values().all(|c| c.status == "not_found"));
    }
}
