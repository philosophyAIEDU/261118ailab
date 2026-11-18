//! 문헌고찰 지원: 선택한 논문으로 주제별 분류, 연구 흐름·방법론·결과·한계 비교, 연구 공백 "후보",
//! 문헌고찰 개요 초안을 만든다.
//!
//! - 결과물은 **검토용 초안**이다. 최종 원고가 아니다.
//! - 연구 공백은 "선택한 자료 범위에서 도출한 후보"로만 표시한다.
//! - 참고문헌 목록은 AI가 만들지 않고, 선택한 논문의 저장된 서지정보로만 만든다.
use crate::error::{AppError, AppResult};
use crate::library::Paper;
use crate::llm::Llm;
use crate::ollama::ChatMessage;
use crate::rag::{self, Citation};
use crate::search;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub label: String,
    /// keyword / tag / field / none — 분류 근거
    pub basis: String,
    pub paper_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    pub id: String,
    pub title: String,
    pub topic: String,
    pub paper_ids: Vec<String>,
    pub groups: Vec<Group>,
    pub draft: String,
    pub citations: Vec<CitationLite>,
    pub removed_markers: usize,
    pub removed_references: Vec<String>,
    pub references: Vec<String>,
    pub model: String,
    pub created_at: String,
    pub notice: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationLite {
    pub label: String,
    pub paper_id: String,
    pub paper_title: String,
    pub page: Option<i64>,
    pub chunk_id: i64,
    pub support: String,
    pub quote: String,
}

impl From<&Citation> for CitationLite {
    fn from(c: &Citation) -> Self {
        Self { label: c.label.clone(), paper_id: c.source.paper_id.clone(), paper_title: c.source.paper_title.clone(), page: c.source.page, chunk_id: c.source.chunk_id, support: c.support.clone(), quote: crate::text::preview(&c.source.text, 200) }
    }
}

pub const DRAFT_NOTICE: &str = "AI가 선택한 자료만으로 만든 검토용 초안입니다. 최종 원고가 아니며, 모든 내용과 인용을 원문과 대조해 확인해 주세요. 연구 공백은 선택한 자료 범위에서 도출한 후보입니다.";

/// APA 7판에 가까운 형식 (저장된 서지정보만 사용, 비어 있는 값은 생략)
pub fn format_reference(p: &Paper) -> String {
    let authors = match p.authors.len() {
        0 => String::new(),
        1 => p.authors[0].clone(),
        2 => format!("{}, & {}", p.authors[0], p.authors[1]),
        n if n <= 20 => format!("{}, & {}", p.authors[..n - 1].join(", "), p.authors[n - 1]),
        _ => format!("{}, ... {}", p.authors[..19].join(", "), p.authors.last().unwrap()),
    };
    let year = p.year.map(|y| format!("({y}).")).unwrap_or_else(|| "(n.d.).".into());
    let mut s = if authors.is_empty() { format!("{} {year}", p.title) } else { format!("{authors} {year} {}.", p.title.trim_end_matches('.')) };
    if !p.journal.is_empty() {
        s.push_str(&format!(" {}.", p.journal));
    }
    if !p.doi.is_empty() {
        s.push_str(&format!(" https://doi.org/{}", p.doi));
    }
    s
}

/// 연구 주제별 분류 (규칙 기반, AI 사용 안 함): 선택 논문 사이에서 가장 많이 공유되는 키워드·태그·분야로 묶는다.
pub fn group_papers(papers: &[Paper]) -> Vec<Group> {
    let mut freq: BTreeMap<String, (String, String, Vec<String>)> = BTreeMap::new();
    for p in papers {
        let mut labels: Vec<(String, String)> = p.keywords.iter().map(|k| (k.clone(), "keyword".to_string())).collect();
        labels.extend(p.tags.iter().map(|t| (t.clone(), "tag".to_string())));
        if !p.field.is_empty() {
            labels.push((p.field.clone(), "field".into()));
        }
        for (l, b) in labels {
            let e = freq.entry(l.trim().to_lowercase()).or_insert_with(|| (l.trim().to_string(), b, vec![]));
            if !e.2.contains(&p.id) {
                e.2.push(p.id.clone());
            }
        }
    }
    let mut assigned: BTreeMap<String, String> = BTreeMap::new();
    let mut cands: Vec<(String, String, Vec<String>)> = freq.into_values().filter(|v| v.2.len() >= 2 && v.2.len() < papers.len().max(3)).collect();
    cands.sort_by(|a, b| b.2.len().cmp(&a.2.len()).then(a.0.cmp(&b.0)));
    let mut groups: Vec<Group> = Vec::new();
    for (label, basis, ids) in cands {
        let free: Vec<String> = ids.into_iter().filter(|i| !assigned.contains_key(i)).collect();
        if free.len() >= 2 {
            for i in &free {
                assigned.insert(i.clone(), label.clone());
            }
            groups.push(Group { label, basis, paper_ids: free });
        }
    }
    let rest: Vec<String> = papers.iter().filter(|p| !assigned.contains_key(&p.id)).map(|p| p.id.clone()).collect();
    if !rest.is_empty() {
        groups.push(Group { label: "기타 (공유 키워드 없음)".into(), basis: "none".into(), paper_ids: rest });
    }
    groups
}

pub fn messages(topic: &str, papers: &[Paper], groups: &[Group], sources: &[rag::Source]) -> Vec<ChatMessage> {
    let mut plist = String::new();
    for (i, p) in papers.iter().enumerate() {
        plist.push_str(&format!("- P{}: {} ({})\n", i + 1, p.title, p.year.map(|y| y.to_string()).unwrap_or_else(|| "연도 미상".into())));
    }
    let mut glist = String::new();
    for g in groups {
        let titles: Vec<String> = g.paper_ids.iter().filter_map(|id| papers.iter().position(|p| &p.id == id).map(|i| format!("P{}", i + 1))).collect();
        glist.push_str(&format!("- {}: {}\n", g.label, titles.join(", ")));
    }
    vec![
        ChatMessage { role: "system".into(), content: rag::SYSTEM_PROMPT.into() },
        ChatMessage {
            role: "user".into(),
            content: format!(
                "문헌고찰 주제: {topic}\n\n선택한 논문:\n{plist}\n규칙 기반 주제 분류:\n{glist}\n발췌 자료:\n{}\n위 자료만 근거로 다음 Markdown 구조의 문헌고찰 개요 초안을 작성하세요. 모든 주장 끝에 [S번호]를 붙이세요.\n## 1. 공통 연구 흐름\n## 2. 연구 방법 비교\n## 3. 연구 결과 비교\n## 4. 연구 한계\n## 5. 연구 공백 후보 (선택한 자료 범위 기준)\n## 6. 문헌고찰 개요 (절 구성안)\n참고문헌 목록은 쓰지 마세요(프로그램이 서지정보로 만듭니다). 자료에 없는 연구를 언급하지 마세요.",
                rag::source_block(sources)
            ),
        },
    ]
}

pub async fn run(db: &Mutex<Connection>, llm: &Llm, paper_ids: &[String], topic: &str, cancel: Arc<AtomicBool>, on_token: impl FnMut(&str) + Send) -> AppResult<Review> {
    if paper_ids.len() < 2 {
        return Err(AppError::Invalid("문헌고찰에는 논문을 2편 이상 선택해 주세요".into()));
    }
    if paper_ids.len() > 15 {
        return Err(AppError::Invalid("한 번에 최대 15편까지 선택할 수 있습니다".into()));
    }
    let topic = if topic.trim().is_empty() { "선택한 논문들의 연구 동향" } else { topic.trim() };
    let (papers, hits) = {
        let conn = db.lock().map_err(|_| AppError::Db("DB 잠금 오류".into()))?;
        let papers: Vec<Paper> = paper_ids.iter().map(|id| crate::library::get_paper(&conn, id)).collect::<AppResult<_>>()?;
        let per = if papers.len() > 8 { 2 } else { 3 };
        let mut hits = Vec::new();
        for p in &papers {
            let r = search::search(&conn, &search::SearchParams { query: &format!("{topic} 연구 방법 결과 한계"), mode: search::Mode::Keyword, limit: per, per_paper: per, papers: Some(std::slice::from_ref(&p.id)) }, None, None)?;
            let mut got = r.hits;
            if got.is_empty() {
                // 검색어와 겹치지 않으면 논문 앞부분(초록)을 사용
                got = crate::compare::gather(&conn, &p.id, &[], 1)?;
            }
            hits.extend(got);
        }
        (papers, hits)
    };
    if hits.is_empty() {
        return Err(AppError::Invalid("선택한 논문에서 본문을 찾지 못했습니다. 본문이 추출된 논문을 선택해 주세요.".into()));
    }
    let groups = group_papers(&papers);
    let sources = rag::to_sources(&hits);
    let raw = llm.chat(&messages(topic, &papers, &groups, &sources), 0.3, Some(16384), false, cancel, on_token).await?;
    let v = rag::verify(&raw, &sources);
    let r = Review {
        id: crate::new_id(),
        title: format!("문헌고찰 개요 — {topic}"),
        topic: topic.to_string(),
        paper_ids: paper_ids.to_vec(),
        groups,
        draft: v.answer,
        citations: v.citations.iter().map(CitationLite::from).collect(),
        removed_markers: v.removed_markers,
        removed_references: v.removed_references,
        references: { let mut r: Vec<String> = papers.iter().map(format_reference).collect(); r.sort(); r },
        model: llm.label(),
        created_at: crate::now(),
        notice: DRAFT_NOTICE.into(),
    };
    let conn = db.lock().map_err(|_| AppError::Db("DB 잠금 오류".into()))?;
    conn.execute(
        "INSERT INTO reviews(id,title,topic,paper_ids,content,model,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![r.id, r.title, r.topic, serde_json::to_string(&r.paper_ids)?, serde_json::to_string(&r)?, r.model, r.created_at],
    )?;
    Ok(r)
}

pub fn list(conn: &Connection) -> AppResult<Vec<Review>> {
    let mut st = conn.prepare("SELECT content FROM reviews ORDER BY created_at DESC")?;
    let v = st.query_map([], |r| r.get::<_, String>(0))?.filter_map(|s| s.ok().and_then(|s| serde_json::from_str(&s).ok())).collect();
    Ok(v)
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Review> {
    let s: String = conn.query_row("SELECT content FROM reviews WHERE id=?1", [id], |r| r.get(0)).map_err(|_| AppError::NotFound("문헌고찰 초안을 찾을 수 없습니다".into()))?;
    Ok(serde_json::from_str(&s)?)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM reviews WHERE id=?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(id: &str, kw: &[&str], authors: &[&str], year: Option<i32>) -> Paper {
        Paper { id: id.into(), title: format!("Title {id}"), keywords: kw.iter().map(|s| s.to_string()).collect(), authors: authors.iter().map(|s| s.to_string()).collect(), year, ..Default::default() }
    }
    #[test]
    fn grouping_and_references() {
        let ps = vec![p("a", &["AI", "writing"], &["Kim, A."], Some(2024)), p("b", &["AI"], &["Lee, B.", "Park, C."], Some(2023)), p("c", &["writing", "feedback"], &[], None), p("d", &["physics"], &[], None)];
        let g = group_papers(&ps);
        assert_eq!(g[0].paper_ids.len(), 2);
        assert!(g.iter().any(|x| x.basis == "none" && x.paper_ids.contains(&"d".to_string())));
        let total: usize = g.iter().map(|x| x.paper_ids.len()).sum();
        assert_eq!(total, 4, "모든 논문이 정확히 한 그룹에");
        assert_eq!(format_reference(&ps[1]), "Lee, B., & Park, C. (2023). Title b.");
        assert_eq!(format_reference(&ps[2]), "Title c (n.d.).");
    }
}
