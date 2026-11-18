//! 연구자료 검색: 키워드(FTS5 BM25), 의미(임베딩 코사인 유사도), 하이브리드(RRF 결합).
//!
//! 검색 결과는 항상 실제 DB의 청크(문단 묶음) ID·논문 ID·쪽 번호를 담는다.
//! 이 값이 AI 답변의 출처(인용)가 되며, 출처를 클릭하면 해당 PDF 쪽이 열린다.
use crate::error::AppResult;
use crate::text;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Keyword,
    Semantic,
    #[default]
    Hybrid,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub chunk_id: i64,
    pub paper_id: String,
    pub paper_title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub page: Option<i64>,
    pub text: String,
    pub score: f64,
    pub keyword_rank: Option<usize>,
    pub vector_rank: Option<usize>,
    pub vector_sim: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchOutcome {
    pub hits: Vec<Hit>,
    pub mode_used: Mode,
    pub terms: Vec<String>,
    /// 의미 검색을 요청했지만 쓸 수 없었던 경우의 이유
    pub notice: Option<String>,
}

fn hydrate(conn: &Connection, ids: &[i64]) -> AppResult<HashMap<i64, Hit>> {
    let mut out = HashMap::new();
    let mut st = conn.prepare_cached("SELECT c.id, c.paper_id, p.title, p.authors, p.year, c.page, c.text FROM chunks c JOIN papers p ON p.id=c.paper_id WHERE c.id=?1")?;
    for id in ids {
        if let Ok(h) = st.query_row([id], |r| {
            Ok(Hit {
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
        }) {
            out.insert(*id, h);
        }
    }
    Ok(out)
}

fn paper_clause(papers: Option<&[String]>) -> String {
    match papers {
        Some(ids) if !ids.is_empty() => {
            let list: Vec<String> = ids.iter().map(|i| format!("'{}'", i.replace('\'', ""))).collect();
            format!(" AND c.paper_id IN ({})", list.join(","))
        }
        _ => String::new(),
    }
}

/// 키워드 검색 → (chunk_id, bm25 점수) 순위 목록
pub fn keyword_ids(conn: &Connection, terms: &[String], limit: usize, papers: Option<&[String]>) -> AppResult<Vec<(i64, f64)>> {
    let Some(q) = text::fts_query(terms) else { return Ok(vec![]) };
    let sql = format!(
        "SELECT c.id, bm25(chunks_fts) AS s FROM chunks_fts JOIN chunks c ON c.id = chunks_fts.rowid WHERE chunks_fts MATCH ?1{} ORDER BY s LIMIT ?2",
        paper_clause(papers)
    );
    let mut st = conn.prepare(&sql)?;
    let v = st.query_map(params![q, limit as i64], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?)))?.collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

/// 메모리에 올린 벡터 색인 (같은 임베딩 모델로 만든 벡터만)
#[derive(Default)]
pub struct VectorIndex {
    pub model: String,
    pub ids: Vec<i64>,
    pub paper_of: Vec<String>,
    pub vecs: Vec<Vec<f32>>,
    pub revision: i64,
}

pub fn normalize_vec(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        v.iter_mut().for_each(|x| *x /= n);
    }
}

pub fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

pub fn blob_to_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

/// 색인 변경 감지용 값 (임베딩 수 + 최대 chunk id)
pub fn embedding_revision(conn: &Connection, model: &str) -> AppResult<i64> {
    Ok(conn.query_row("SELECT count(*) * 1000003 + coalesce(max(chunk_id),0) FROM embeddings WHERE model=?1", [model], |r| r.get(0))?)
}

pub fn load_index(conn: &Connection, model: &str) -> AppResult<VectorIndex> {
    let mut st = conn.prepare("SELECT e.chunk_id, c.paper_id, e.vec FROM embeddings e JOIN chunks c ON c.id=e.chunk_id WHERE e.model=?1")?;
    let mut idx = VectorIndex { model: model.into(), revision: embedding_revision(conn, model)?, ..Default::default() };
    let rows = st.query_map([model], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Vec<u8>>(2)?)))?;
    for row in rows {
        let (id, pid, b) = row?;
        idx.ids.push(id);
        idx.paper_of.push(pid);
        idx.vecs.push(blob_to_vec(&b));
    }
    Ok(idx)
}

pub fn vector_ids(idx: &VectorIndex, q: &[f32], limit: usize, papers: Option<&[String]>) -> Vec<(i64, f32)> {
    let mut qn = q.to_vec();
    normalize_vec(&mut qn);
    let mut scored: Vec<(i64, f32)> = idx
        .vecs
        .iter()
        .enumerate()
        .filter(|(i, v)| v.len() == qn.len() && papers.map(|p| p.is_empty() || p.contains(&idx.paper_of[*i])).unwrap_or(true))
        .map(|(i, v)| (idx.ids[i], v.iter().zip(&qn).map(|(a, b)| a * b).sum::<f32>()))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    scored
}

pub struct SearchParams<'a> {
    pub query: &'a str,
    pub mode: Mode,
    pub limit: usize,
    pub per_paper: usize,
    pub papers: Option<&'a [String]>,
}

/// 하이브리드 검색: 두 순위를 RRF(Reciprocal Rank Fusion)로 결합한 뒤,
/// 한 논문이 결과를 독점하지 않도록 논문당 개수를 제한한다(재정렬).
pub fn search(conn: &Connection, p: &SearchParams, index: Option<&VectorIndex>, qvec: Option<&[f32]>) -> AppResult<SearchOutcome> {
    let mut terms = text::query_terms(p.query);
    if terms.is_empty() && !p.query.trim().is_empty() {
        terms = vec![p.query.trim().to_string()];
    }
    let mut notice = None;
    let can_vec = index.map(|i| !i.ids.is_empty()).unwrap_or(false) && qvec.is_some();
    let mode = match p.mode {
        Mode::Keyword => Mode::Keyword,
        m if !can_vec => {
            if m != Mode::Keyword {
                notice = Some(if index.map(|i| i.ids.is_empty()).unwrap_or(true) {
                    "의미 검색 색인이 아직 없어 키워드 검색으로 찾았습니다. 설정 > AI 모델에서 '의미 검색 색인 만들기'를 실행하면 더 정확해집니다.".to_string()
                } else {
                    "임베딩 모델을 사용할 수 없어 키워드 검색으로 찾았습니다.".to_string()
                });
            }
            Mode::Keyword
        }
        m => m,
    };
    let pool = p.limit * 4;
    let kw = if mode != Mode::Semantic { keyword_ids(conn, &terms, pool, p.papers)? } else { vec![] };
    let vs = if mode != Mode::Keyword { vector_ids(index.unwrap(), qvec.unwrap(), pool, p.papers) } else { vec![] };
    const K: f64 = 60.0;
    let mut fused: HashMap<i64, (f64, Option<usize>, Option<usize>, Option<f32>)> = HashMap::new();
    for (rank, (id, _)) in kw.iter().enumerate() {
        let e = fused.entry(*id).or_default();
        e.0 += 1.0 / (K + rank as f64 + 1.0);
        e.1 = Some(rank + 1);
    }
    for (rank, (id, sim)) in vs.iter().enumerate() {
        // 유사도가 너무 낮은 것은 근거로 쓰지 않는다
        if *sim < 0.25 {
            continue;
        }
        let e = fused.entry(*id).or_default();
        e.0 += 1.0 / (K + rank as f64 + 1.0);
        e.2 = Some(rank + 1);
        e.3 = Some(*sim);
    }
    let mut order: Vec<(i64, (f64, Option<usize>, Option<usize>, Option<f32>))> = fused.into_iter().collect();
    order.sort_by(|a, b| b.1 .0.partial_cmp(&a.1 .0).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
    let ids: Vec<i64> = order.iter().map(|o| o.0).collect();
    let mut rows = hydrate(conn, &ids)?;
    let mut per: HashMap<String, usize> = HashMap::new();
    let mut hits = Vec::new();
    for (id, (score, kr, vr, sim)) in order {
        let Some(mut h) = rows.remove(&id) else { continue };
        let n = per.entry(h.paper_id.clone()).or_default();
        if *n >= p.per_paper {
            continue;
        }
        *n += 1;
        h.score = score;
        h.keyword_rank = kr;
        h.vector_rank = vr;
        h.vector_sim = sim;
        hits.push(h);
        if hits.len() >= p.limit {
            break;
        }
    }
    Ok(SearchOutcome { hits, mode_used: mode, terms, notice })
}

/// 임베딩이 없는(또는 다른 모델로 만든) 청크
pub fn pending_chunks(conn: &Connection, model: &str, limit: usize) -> AppResult<Vec<(i64, String, String)>> {
    let mut st = conn.prepare(
        "SELECT c.id, p.title, c.text FROM chunks c JOIN papers p ON p.id=c.paper_id LEFT JOIN embeddings e ON e.chunk_id=c.id WHERE e.chunk_id IS NULL OR e.model<>?1 ORDER BY c.id LIMIT ?2",
    )?;
    let v = st.query_map(params![model, limit as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
    Ok(v)
}

pub fn count_pending(conn: &Connection, model: &str) -> AppResult<i64> {
    Ok(conn.query_row("SELECT count(*) FROM chunks c LEFT JOIN embeddings e ON e.chunk_id=c.id WHERE e.chunk_id IS NULL OR e.model<>?1", [model], |r| r.get(0))?)
}

pub fn store_embeddings(conn: &mut Connection, model: &str, items: &[(i64, Vec<f32>)]) -> AppResult<()> {
    let tx = conn.transaction()?;
    {
        let mut st = tx.prepare_cached("INSERT OR REPLACE INTO embeddings(chunk_id, model, dim, vec) VALUES (?1,?2,?3,?4)")?;
        for (id, v) in items {
            let mut v = v.clone();
            normalize_vec(&mut v);
            st.execute(params![id, model, v.len() as i64, vec_to_blob(&v)])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// 임베딩 입력 문자열: 논문 제목을 앞에 붙여 맥락을 준다
pub fn embed_input(title: &str, chunk: &str) -> String {
    format!("{}\n{}", text::preview(title, 120), chunk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::{lib, write_pdf};

    #[test]
    fn keyword_and_hybrid_search_with_pages() {
        let (d, paths, mut c) = lib();
        let a = write_pdf(d.path(), "a.pdf", &["Intro about seminars", "Students used generative AI tools and critical thinking scores improved"], Some("Paper A"));
        let b = write_pdf(d.path(), "b.pdf", &["Critical thinking in physics labs", "No AI was used here at all in this lab"], Some("Paper B"));
        let ida = crate::library::import_file(&mut c, &paths, &a).unwrap().paper_id.unwrap();
        let _idb = crate::library::import_file(&mut c, &paths, &b).unwrap().paper_id.unwrap();
        let p = SearchParams { query: "generative AI critical thinking", mode: Mode::Hybrid, limit: 5, per_paper: 3, papers: None };
        let r = search(&c, &p, None, None).unwrap();
        assert_eq!(r.mode_used, Mode::Keyword);
        assert!(r.notice.is_some());
        assert_eq!(r.hits[0].paper_id, ida);
        assert_eq!(r.hits[0].page, Some(2));
        // 벡터: 가짜 임베딩 (청크 id에 따라 방향 지정)
        let chunks: Vec<i64> = c.prepare("SELECT id FROM chunks ORDER BY id").unwrap().query_map([], |r| r.get(0)).unwrap().map(|x| x.unwrap()).collect();
        let items: Vec<(i64, Vec<f32>)> = chunks.iter().enumerate().map(|(i, id)| (*id, if i == 3 { vec![1.0, 0.0] } else { vec![0.0, 1.0] })).collect();
        store_embeddings(&mut c, "fake", &items).unwrap();
        assert_eq!(count_pending(&c, "fake").unwrap(), 0);
        assert_eq!(count_pending(&c, "other").unwrap(), chunks.len() as i64);
        let idx = load_index(&c, "fake").unwrap();
        let p = SearchParams { query: "zzzz", mode: Mode::Semantic, limit: 2, per_paper: 3, papers: None };
        let r = search(&c, &p, Some(&idx), Some(&[1.0, 0.0])).unwrap();
        assert_eq!(r.mode_used, Mode::Semantic);
        assert_eq!(r.hits[0].chunk_id, chunks[3]);
        // 논문 범위 제한
        let only = vec![ida.clone()];
        let p = SearchParams { query: "critical thinking", mode: Mode::Keyword, limit: 5, per_paper: 5, papers: Some(&only) };
        let r = search(&c, &p, None, None).unwrap();
        assert!(r.hits.iter().all(|h| h.paper_id == ida));
    }
}
