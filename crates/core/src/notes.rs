//! 연구 노트: Markdown 본문, 논문 연결, 태그, [[노트 제목]] 형식의 링크와 백링크, 검색.
use crate::error::{AppError, AppResult};
use crate::text;
use regex::Regex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub body: String,
    pub project_id: Option<String>,
    pub origin: String,
    pub tags: Vec<String>,
    pub paper_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NoteRef {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NoteDetail {
    #[serde(flatten)]
    pub note: Note,
    pub backlinks: Vec<NoteRef>,
    pub outgoing: Vec<NoteRef>,
    /// 링크했지만 해당 제목의 노트가 없는 경우
    pub unresolved: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NoteInput {
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub paper_ids: Vec<String>,
    /// AI 초안을 교수님이 검토한 뒤 저장하는 경우 true
    #[serde(default)]
    pub from_ai_draft: bool,
}

fn wiki_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\[\[([^\[\]\n|]{1,200})(?:\|[^\[\]\n]*)?\]\]").unwrap())
}

pub fn wiki_links(body: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for c in wiki_re().captures_iter(body) {
        let t = c[1].trim().to_string();
        if !v.contains(&t) {
            v.push(t);
        }
    }
    v
}

fn load(conn: &Connection, id: &str) -> AppResult<Note> {
    let mut n = conn
        .query_row("SELECT id,title,body,project_id,origin,created_at,updated_at FROM notes WHERE id=?1", [id], |r| {
            Ok(Note { id: r.get(0)?, title: r.get(1)?, body: r.get(2)?, project_id: r.get(3)?, origin: r.get(4)?, created_at: r.get(5)?, updated_at: r.get(6)?, ..Default::default() })
        })
        .map_err(|_| AppError::NotFound("노트를 찾을 수 없습니다".into()))?;
    let mut st = conn.prepare_cached("SELECT tag FROM note_tags WHERE note_id=?1 ORDER BY tag")?;
    n.tags = st.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let mut st = conn.prepare_cached("SELECT paper_id FROM note_papers WHERE note_id=?1")?;
    n.paper_ids = st.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    Ok(n)
}

fn write_relations(conn: &Connection, id: &str, input: &NoteInput) -> AppResult<()> {
    conn.execute("DELETE FROM note_tags WHERE note_id=?1", [id])?;
    for t in input.tags.iter().map(|t| t.trim().trim_start_matches('#')).filter(|t| !t.is_empty()) {
        conn.execute("INSERT OR IGNORE INTO note_tags(note_id, tag) VALUES (?1,?2)", params![id, t])?;
    }
    conn.execute("DELETE FROM note_papers WHERE note_id=?1", [id])?;
    for p in &input.paper_ids {
        conn.execute("INSERT OR IGNORE INTO note_papers(note_id, paper_id) SELECT ?1, id FROM papers WHERE id=?2", params![id, p])?;
    }
    conn.execute("DELETE FROM notes_fts WHERE note_id=?1", [id])?;
    conn.execute("INSERT INTO notes_fts(note_id, tokens) VALUES (?1,?2)", params![id, text::index_string(&format!("{} {} {}", input.title, input.body, input.tags.join(" ")))])?;
    Ok(())
}

pub fn create(conn: &mut Connection, input: &NoteInput) -> AppResult<NoteDetail> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(AppError::Invalid("노트 제목을 입력해 주세요".into()));
    }
    let id = crate::new_id();
    let now = crate::now();
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO notes(id,title,body,project_id,origin,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?6)",
        params![id, title, input.body, input.project_id.as_ref().filter(|s| !s.is_empty()), if input.from_ai_draft { "ai_reviewed" } else { "user" }, now],
    )?;
    write_relations(&tx, &id, input)?;
    tx.commit()?;
    get(conn, &id)
}

pub fn update(conn: &mut Connection, id: &str, input: &NoteInput) -> AppResult<NoteDetail> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(AppError::Invalid("노트 제목을 입력해 주세요".into()));
    }
    let tx = conn.transaction()?;
    let n = tx.execute(
        "UPDATE notes SET title=?2, body=?3, project_id=?4, updated_at=?5 WHERE id=?1",
        params![id, title, input.body, input.project_id.as_ref().filter(|s| !s.is_empty()), crate::now()],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("노트를 찾을 수 없습니다".into()));
    }
    write_relations(&tx, id, input)?;
    tx.commit()?;
    get(conn, id)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM notes_fts WHERE note_id=?1", [id])?;
    conn.execute("DELETE FROM links WHERE (src_type='note' AND src_id=?1) OR (dst_type='note' AND dst_id=?1)", [id])?;
    conn.execute("DELETE FROM notes WHERE id=?1", [id])?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> AppResult<NoteDetail> {
    let note = load(conn, id)?;
    let all: Vec<(String, String, String)> = {
        let mut st = conn.prepare("SELECT id, title, body FROM notes")?;
        let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
        v
    };
    let me = note.title.to_lowercase();
    let backlinks = all
        .iter()
        .filter(|(oid, _, body)| oid != id && wiki_links(body).iter().any(|l| l.to_lowercase() == me))
        .map(|(oid, t, _)| NoteRef { id: oid.clone(), title: t.clone() })
        .collect();
    let mut outgoing = vec![];
    let mut unresolved = vec![];
    for l in wiki_links(&note.body) {
        match all.iter().find(|(_, t, _)| t.to_lowercase() == l.to_lowercase()) {
            Some((oid, t, _)) => outgoing.push(NoteRef { id: oid.clone(), title: t.clone() }),
            None => unresolved.push(l),
        }
    }
    Ok(NoteDetail { note, backlinks, outgoing, unresolved })
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NoteFilter {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub paper_id: Option<String>,
}

pub fn list(conn: &Connection, f: &NoteFilter) -> AppResult<Vec<Note>> {
    let mut sql = "SELECT id FROM notes WHERE 1=1".to_string();
    let mut args: Vec<String> = vec![];
    if !f.query.trim().is_empty() {
        let mut terms = text::query_terms(&f.query);
        if terms.is_empty() {
            terms.push(f.query.trim().to_string());
        }
        match text::fts_query(&terms) {
            Some(q) => {
                sql.push_str(" AND id IN (SELECT note_id FROM notes_fts WHERE notes_fts MATCH ?)");
                args.push(q);
            }
            None => return Ok(vec![]),
        }
    }
    if let Some(t) = f.tag.as_ref().filter(|s| !s.is_empty()) {
        sql.push_str(" AND id IN (SELECT note_id FROM note_tags WHERE tag=?)");
        args.push(t.clone());
    }
    if let Some(p) = f.project_id.as_ref().filter(|s| !s.is_empty()) {
        sql.push_str(" AND project_id=?");
        args.push(p.clone());
    }
    if let Some(p) = f.paper_id.as_ref().filter(|s| !s.is_empty()) {
        sql.push_str(" AND id IN (SELECT note_id FROM note_papers WHERE paper_id=?)");
        args.push(p.clone());
    }
    sql.push_str(" ORDER BY updated_at DESC");
    let mut st = conn.prepare(&sql)?;
    let ids: Vec<String> = st.query_map(rusqlite::params_from_iter(args.iter()), |r| r.get(0))?.collect::<Result<_, _>>()?;
    ids.iter().map(|id| load(conn, id)).collect()
}

/// AI 연구 노트 초안용 프롬프트 (결과는 저장하지 않고 화면에 초안으로만 보여 준다)
pub fn draft_messages(paper_title: &str, sources: &[crate::rag::Source], focus: &str) -> Vec<crate::ollama::ChatMessage> {
    use crate::ollama::ChatMessage;
    let focus = if focus.trim().is_empty() { "이 논문의 핵심 주장, 연구 방법, 주요 결과, 한계, 내 연구에 주는 시사점" } else { focus };
    vec![
        ChatMessage { role: "system".into(), content: crate::rag::SYSTEM_PROMPT.into() },
        ChatMessage {
            role: "user".into(),
            content: format!(
                "다음은 논문 「{}」의 일부입니다.\n\n{}\n위 자료만 근거로 연구 노트 초안을 Markdown으로 작성하세요. 다룰 내용: {}.\n각 항목 끝에 근거 자료 번호 [S번호]를 표시하고, 자료에서 확인되지 않는 내용은 '원문에서 확인되지 않음'이라고 쓰세요.",
                paper_title,
                crate::rag::source_block(sources),
                focus
            ),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::lib;

    #[test]
    fn notes_links_backlinks_search() {
        let (_d, _p, mut c) = lib();
        let a = create(&mut c, &NoteInput { title: "비판적 사고 정의".into(), body: "Ennis의 정의 정리".into(), project_id: None, tags: vec!["#개념".into()], paper_ids: vec!["missing".into()], from_ai_draft: false }).unwrap();
        assert_eq!(a.note.tags, vec!["개념"]);
        assert!(a.note.paper_ids.is_empty(), "존재하지 않는 논문은 연결되지 않음");
        let b = create(&mut c, &NoteInput { title: "AI 글쓰기 수업 관찰".into(), body: "[[비판적 사고 정의]]와 연결. [[없는 노트]]".into(), project_id: None, tags: vec![], paper_ids: vec![], from_ai_draft: true }).unwrap();
        assert_eq!(b.note.origin, "ai_reviewed");
        assert_eq!(b.outgoing.len(), 1);
        assert_eq!(b.unresolved, vec!["없는 노트"]);
        let a2 = get(&c, &a.note.id).unwrap();
        assert_eq!(a2.backlinks.len(), 1);
        assert_eq!(list(&c, &NoteFilter { query: "글쓰기".into(), ..Default::default() }).unwrap().len(), 1);
        assert_eq!(list(&c, &NoteFilter { tag: Some("개념".into()), ..Default::default() }).unwrap().len(), 1);
        let upd = update(&mut c, &a.note.id, &NoteInput { title: "비판적 사고 정의".into(), body: "수정됨 Facione".into(), project_id: None, tags: vec![], paper_ids: vec![], from_ai_draft: false }).unwrap();
        assert!(upd.note.body.contains("Facione"));
        assert_eq!(list(&c, &NoteFilter { query: "Facione".into(), ..Default::default() }).unwrap().len(), 1);
        delete(&c, &a.note.id).unwrap();
        assert_eq!(list(&c, &NoteFilter::default()).unwrap().len(), 1);
    }
}
