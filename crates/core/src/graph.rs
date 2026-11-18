//! 지식 그래프: 논문·노트·주제·핵심 개념·프로젝트의 관계.
//!
//! 연결 종류는 섞지 않고 구분한다.
//! - `citation`     : 실제 인용 관계 (본문/참고문헌에서 DOI·제목이 확인됨, 근거 쪽 포함)
//! - `topic`        : 같은 연구 주제(연구 분야·태그)
//! - `keyword`      : 공통 키워드(핵심 개념)
//! - `user`         : 교수님이 직접 만든 연결 (노트-논문, 프로젝트-논문, 노트 [[링크]], 수동 연결)
//! - `ai_suggested` : AI(임베딩 유사도)가 제안한 관련성 — 확인 전까지는 제안일 뿐이다
use crate::error::{AppError, AppResult};
use crate::search;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub id: String,
    /// paper / note / project / topic / concept
    pub kind: String,
    pub label: String,
    pub ref_id: String,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub kind: String,
    pub label: String,
    pub score: Option<f64>,
    pub page: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

fn norm_kw(s: &str) -> String {
    s.trim().to_lowercase()
}

pub fn build(conn: &Connection) -> AppResult<Graph> {
    let mut nodes: BTreeMap<String, Node> = BTreeMap::new();
    let mut edges: Vec<Edge> = Vec::new();
    let push_edge = |edges: &mut Vec<Edge>, s: &str, t: &str, kind: &str, label: &str, score: Option<f64>, page: Option<i64>| {
        edges.push(Edge { id: format!("{kind}:{s}->{t}"), source: s.into(), target: t.into(), kind: kind.into(), label: label.into(), score, page });
    };
    // 논문
    let papers: Vec<(String, String, String, String, Option<i32>)> = conn
        .prepare("SELECT id, title, keywords, field, year FROM papers")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
        .collect::<Result<_, _>>()?;
    let mut kw_papers: HashMap<String, (String, Vec<String>)> = HashMap::new();
    for (id, title, kws, field, _y) in &papers {
        nodes.insert(format!("paper:{id}"), Node { id: format!("paper:{id}"), kind: "paper".into(), label: title.clone(), ref_id: id.clone(), weight: 1.0 });
        for k in serde_json::from_str::<Vec<String>>(kws).unwrap_or_default() {
            let e = kw_papers.entry(norm_kw(&k)).or_insert_with(|| (k.trim().to_string(), vec![]));
            if !e.1.contains(id) {
                e.1.push(id.clone());
            }
        }
        if !field.trim().is_empty() {
            let tid = format!("topic:{}", norm_kw(field));
            nodes.entry(tid.clone()).or_insert(Node { id: tid.clone(), kind: "topic".into(), label: field.trim().into(), ref_id: field.trim().into(), weight: 0.0 }).weight += 1.0;
            push_edge(&mut edges, &format!("paper:{id}"), &tid, "topic", "연구 분야", None, None);
        }
    }
    // 태그 = 주제
    for (pid, tag) in conn.prepare("SELECT paper_id, tag FROM paper_tags")?.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.collect::<Result<Vec<_>, _>>()? {
        let tid = format!("topic:{}", norm_kw(&tag));
        nodes.entry(tid.clone()).or_insert(Node { id: tid.clone(), kind: "topic".into(), label: tag.clone(), ref_id: tag.clone(), weight: 0.0 }).weight += 1.0;
        push_edge(&mut edges, &format!("paper:{pid}"), &tid, "topic", "주제 태그", None, None);
    }
    // 공통 키워드 → 핵심 개념 노드 (2편 이상 공유)
    for (k, (label, ps)) in kw_papers.iter().filter(|(_, v)| v.1.len() >= 2) {
        let cid = format!("concept:{k}");
        nodes.insert(cid.clone(), Node { id: cid.clone(), kind: "concept".into(), label: label.clone(), ref_id: label.clone(), weight: ps.len() as f64 });
        for p in ps {
            push_edge(&mut edges, &format!("paper:{p}"), &cid, "keyword", "공통 키워드", None, None);
        }
    }
    // 실제 인용
    for (a, b, ev, page) in conn
        .prepare("SELECT citing_id, cited_id, evidence, page FROM citations")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<i64>>(3)?)))?
        .collect::<Result<Vec<_>, _>>()?
    {
        let label = if ev == "doi" { "인용 (DOI 확인)" } else { "인용 (제목 확인)" };
        push_edge(&mut edges, &format!("paper:{a}"), &format!("paper:{b}"), "citation", label, None, page);
    }
    // 노트
    let notes: Vec<(String, String, Option<String>, String)> =
        conn.prepare("SELECT id, title, project_id, body FROM notes")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?;
    for (id, title, _, _) in &notes {
        nodes.insert(format!("note:{id}"), Node { id: format!("note:{id}"), kind: "note".into(), label: title.clone(), ref_id: id.clone(), weight: 1.0 });
    }
    for (id, _, _, body) in &notes {
        for l in crate::notes::wiki_links(body) {
            if let Some((oid, ..)) = notes.iter().find(|n| n.1.to_lowercase() == l.to_lowercase()) {
                push_edge(&mut edges, &format!("note:{id}"), &format!("note:{oid}"), "user", "노트 링크", None, None);
            }
        }
    }
    for (n, p) in conn.prepare("SELECT note_id, paper_id FROM note_papers")?.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.collect::<Result<Vec<_>, _>>()? {
        push_edge(&mut edges, &format!("note:{n}"), &format!("paper:{p}"), "user", "노트-논문 연결", None, None);
    }
    // 프로젝트
    for (id, name) in conn.prepare("SELECT id, name FROM projects")?.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.collect::<Result<Vec<_>, _>>()? {
        nodes.insert(format!("project:{id}"), Node { id: format!("project:{id}"), kind: "project".into(), label: name, ref_id: id.clone(), weight: 1.0 });
    }
    for (pr, p) in conn.prepare("SELECT project_id, paper_id FROM project_papers")?.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.collect::<Result<Vec<_>, _>>()? {
        push_edge(&mut edges, &format!("project:{pr}"), &format!("paper:{p}"), "user", "프로젝트 자료", None, None);
    }
    for (id, _, pr, _) in &notes {
        if let Some(pr) = pr {
            push_edge(&mut edges, &format!("project:{pr}"), &format!("note:{id}"), "user", "프로젝트 노트", None, None);
        }
    }
    // 저장된 연결 (사용자 / AI 제안)
    for (id, st, si, dt, di, kind, label, score) in conn
        .prepare("SELECT id, src_type, src_id, dst_type, dst_id, kind, label, score FROM links WHERE kind IN ('user','ai_suggested')")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?, r.get::<_, String>(6)?, r.get::<_, Option<f64>>(7)?)))?
        .collect::<Result<Vec<_>, _>>()?
    {
        edges.push(Edge { id: format!("link:{id}"), source: format!("{st}:{si}"), target: format!("{dt}:{di}"), kind, label, score, page: None });
    }
    edges.retain(|e| nodes.contains_key(&e.source) && nodes.contains_key(&e.target));
    Ok(Graph { nodes: nodes.into_values().collect(), edges })
}

pub fn add_user_link(conn: &Connection, src_type: &str, src_id: &str, dst_type: &str, dst_id: &str, label: &str) -> AppResult<()> {
    for t in [src_type, dst_type] {
        if !matches!(t, "paper" | "note" | "project") {
            return Err(AppError::Invalid("연결할 수 없는 항목입니다".into()));
        }
    }
    conn.execute(
        "INSERT INTO links(id,src_type,src_id,dst_type,dst_id,kind,label,created_at) VALUES (?1,?2,?3,?4,?5,'user',?6,?7)",
        params![crate::new_id(), src_type, src_id, dst_type, dst_id, if label.is_empty() { "직접 연결" } else { label }, crate::now()],
    )?;
    Ok(())
}

/// AI 제안 연결 수락 → 사용자 연결로 전환 / 거절 → 다시 제안하지 않도록 기록
pub fn resolve_suggestion(conn: &Connection, link_id: &str, accept: bool) -> AppResult<()> {
    let kind = if accept { "user" } else { "dismissed" };
    let label = if accept { "AI 제안 → 교수님 확인" } else { "거절됨" };
    conn.execute("UPDATE links SET kind=?2, label=?3 WHERE id=?1 AND kind='ai_suggested'", params![link_id, kind, label])?;
    Ok(())
}

pub fn delete_link(conn: &Connection, link_id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM links WHERE id=?1", [link_id])?;
    Ok(())
}

/// 임베딩 기반 관련 논문 제안. 논문별 평균 벡터의 코사인 유사도가 기준 이상인 쌍(논문당 최대 3개).
pub fn suggest_links(conn: &Connection, index: &search::VectorIndex, threshold: f32) -> AppResult<usize> {
    if index.ids.is_empty() {
        return Err(AppError::Invalid("의미 검색 색인이 없어 AI 관련성 제안을 만들 수 없습니다. 먼저 색인을 만들어 주세요.".into()));
    }
    let mut sums: HashMap<&str, (Vec<f32>, usize)> = HashMap::new();
    for (i, v) in index.vecs.iter().enumerate() {
        let e = sums.entry(index.paper_of[i].as_str()).or_insert_with(|| (vec![0.0; v.len()], 0));
        if e.0.len() == v.len() {
            e.0.iter_mut().zip(v).for_each(|(a, b)| *a += b);
            e.1 += 1;
        }
    }
    let mut cents: Vec<(&str, Vec<f32>)> = sums.into_iter().map(|(k, (mut v, _))| {
        search::normalize_vec(&mut v);
        (k, v)
    }).collect();
    cents.sort_by(|a, b| a.0.cmp(b.0));
    let existing: Vec<(String, String)> = conn
        .prepare("SELECT src_id, dst_id FROM links WHERE src_type='paper' AND dst_type='paper'")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let cited: Vec<(String, String)> = conn.prepare("SELECT citing_id, cited_id FROM citations")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    let known = |a: &str, b: &str| {
        existing.iter().chain(cited.iter()).any(|(x, y)| (x == a && y == b) || (x == b && y == a))
    };
    conn.execute("DELETE FROM links WHERE kind='ai_suggested'", [])?;
    let mut n = 0;
    let mut per: HashMap<&str, usize> = HashMap::new();
    let mut pairs = Vec::new();
    for i in 0..cents.len() {
        for j in i + 1..cents.len() {
            let s: f32 = cents[i].1.iter().zip(&cents[j].1).map(|(a, b)| a * b).sum();
            if s >= threshold {
                pairs.push((s, i, j));
            }
        }
    }
    pairs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    for (s, i, j) in pairs {
        let (a, b) = (cents[i].0, cents[j].0);
        if known(a, b) || *per.get(a).unwrap_or(&0) >= 3 || *per.get(b).unwrap_or(&0) >= 3 {
            continue;
        }
        *per.entry(a).or_default() += 1;
        *per.entry(b).or_default() += 1;
        conn.execute(
            "INSERT INTO links(id,src_type,src_id,dst_type,dst_id,kind,label,score,created_at) VALUES (?1,'paper',?2,'paper',?3,'ai_suggested','AI 제안: 내용 유사',?4,?5)",
            params![crate::new_id(), a, b, (s as f64 * 100.0).round() / 100.0, crate::now()],
        )?;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::{lib, write_pdf};

    #[test]
    fn graph_kinds_are_separated() {
        let (d, paths, mut c) = lib();
        let a = write_pdf(d.path(), "a.pdf", &["Foundations of Reflective Judgment in Higher Education\nKeywords: critical thinking, reflection"], Some("Foundations of Reflective Judgment in Higher Education"));
        let b = write_pdf(d.path(), "b.pdf", &["Keywords: critical thinking, AI\nAI tutoring study", "References: Foundations of Reflective Judgment in Higher Education"], Some("AI tutoring study in seminars"));
        let ia = crate::library::import_file(&mut c, &paths, &a).unwrap().paper_id.unwrap();
        let ib = crate::library::import_file(&mut c, &paths, &b).unwrap().paper_id.unwrap();
        let n = crate::notes::create(&mut c, &crate::notes::NoteInput { title: "메모".into(), body: "".into(), project_id: None, tags: vec![], paper_ids: vec![ia.clone()], from_ai_draft: false }).unwrap();
        let g = build(&c).unwrap();
        assert!(g.nodes.iter().any(|n| n.kind == "concept" && n.label == "critical thinking"));
        assert!(g.edges.iter().any(|e| e.kind == "citation" && e.source == format!("paper:{ib}") && e.target == format!("paper:{ia}")));
        assert!(g.edges.iter().any(|e| e.kind == "user" && e.source == format!("note:{}", n.note.id)));
        // AI 제안
        let chunks: Vec<(i64, String)> = c.prepare("SELECT id, paper_id FROM chunks").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(|x| x.unwrap()).collect();
        let items: Vec<(i64, Vec<f32>)> = chunks.iter().map(|(id, _)| (*id, vec![1.0, 0.1])).collect();
        crate::search::store_embeddings(&mut c, "m", &items).unwrap();
        let idx = crate::search::load_index(&c, "m").unwrap();
        // 이미 인용 관계가 있으므로 제안하지 않음
        assert_eq!(suggest_links(&c, &idx, 0.8).unwrap(), 0);
        c.execute("DELETE FROM citations", []).unwrap();
        assert_eq!(suggest_links(&c, &idx, 0.8).unwrap(), 1);
        let g = build(&c).unwrap();
        let s = g.edges.iter().find(|e| e.kind == "ai_suggested").unwrap();
        let link_id = s.id.trim_start_matches("link:").to_string();
        resolve_suggestion(&c, &link_id, false).unwrap();
        let g = build(&c).unwrap();
        assert!(!g.edges.iter().any(|e| e.kind == "ai_suggested"));
        assert_eq!(suggest_links(&c, &idx, 0.8).unwrap(), 0, "거절한 제안은 다시 제안하지 않음");
    }
}
