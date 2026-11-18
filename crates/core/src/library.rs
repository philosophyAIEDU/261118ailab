//! 논문 라이브러리: 등록(가져오기), 중복 검사, 메타데이터 수정, 태그, 재처리, 삭제.
//!
//! - 원본 파일은 읽기만 한다. 연구실 폴더의 `papers/`에 사본을 저장하고 그 사본을 사용한다.
//! - 삭제는 연구실 안의 기록과 사본만 지우며 교수님의 원본 파일은 절대 건드리지 않는다.
use crate::bib::BibEntry;
use crate::error::{AppError, AppResult};
use crate::extract::{self, Extracted};
use crate::metadata::{self, Metadata};
use crate::paths::LibraryPaths;
use crate::text;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const CHUNK_TARGET: usize = 550;
pub const CHUNK_MAX: usize = 900;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Paper {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub journal: String,
    pub doi: String,
    #[serde(rename = "abstract")]
    pub abstract_text: String,
    pub keywords: Vec<String>,
    pub field: String,
    pub memo: String,
    pub tags: Vec<String>,
    pub file_path: Option<String>,
    pub original_name: String,
    pub file_kind: String,
    pub file_size: i64,
    pub page_count: Option<i64>,
    pub text_status: String,
    pub status_detail: String,
    pub meta_source: BTreeMap<String, String>,
    pub bib_key: String,
    pub created_at: String,
    pub updated_at: String,
    pub chunk_count: i64,
}

const PAPER_COLS: &str = "id,title,authors,year,journal,doi,abstract,keywords,field,memo,file_path,original_name,file_kind,file_size,page_count,text_status,status_detail,meta_source,bib_key,created_at,updated_at,(SELECT count(*) FROM chunks c WHERE c.paper_id=papers.id)";

fn row_to_paper(r: &rusqlite::Row) -> rusqlite::Result<Paper> {
    let j = |s: String| serde_json::from_str::<Vec<String>>(&s).unwrap_or_default();
    Ok(Paper {
        id: r.get(0)?,
        title: r.get(1)?,
        authors: j(r.get(2)?),
        year: r.get(3)?,
        journal: r.get(4)?,
        doi: r.get(5)?,
        abstract_text: r.get(6)?,
        keywords: j(r.get(7)?),
        field: r.get(8)?,
        memo: r.get(9)?,
        file_path: r.get(10)?,
        original_name: r.get(11)?,
        file_kind: r.get(12)?,
        file_size: r.get(13)?,
        page_count: r.get(14)?,
        text_status: r.get(15)?,
        status_detail: r.get(16)?,
        meta_source: serde_json::from_str(&r.get::<_, String>(17)?).unwrap_or_default(),
        bib_key: r.get(18)?,
        created_at: r.get(19)?,
        updated_at: r.get(20)?,
        chunk_count: r.get(21)?,
        tags: vec![],
    })
}

fn load_tags(conn: &Connection, p: &mut Paper) -> AppResult<()> {
    let mut st = conn.prepare_cached("SELECT tag FROM paper_tags WHERE paper_id=?1 ORDER BY tag")?;
    p.tags = st.query_map([&p.id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    Ok(())
}

pub fn get_paper(conn: &Connection, id: &str) -> AppResult<Paper> {
    let mut p = conn
        .query_row(&format!("SELECT {PAPER_COLS} FROM papers WHERE id=?1"), [id], row_to_paper)
        .map_err(|_| AppError::NotFound("논문을 찾을 수 없습니다".into()))?;
    load_tags(conn, &mut p)?;
    Ok(p)
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PaperFilter {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    /// recent / title / year
    #[serde(default)]
    pub sort: Option<String>,
}

pub fn list_papers(conn: &Connection, f: &PaperFilter) -> AppResult<Vec<Paper>> {
    let mut sql = format!("SELECT {PAPER_COLS} FROM papers WHERE 1=1");
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![];
    let terms = text::query_terms(&f.query);
    if !f.query.trim().is_empty() {
        // 서지정보 색인 + 본문 색인 모두에서 찾는다
        let q = text::fts_query(&if terms.is_empty() { vec![f.query.trim().to_string()] } else { terms.clone() });
        match q {
            Some(q) => {
                sql.push_str(" AND (id IN (SELECT paper_id FROM papers_fts WHERE papers_fts MATCH ?) OR id IN (SELECT c.paper_id FROM chunks_fts JOIN chunks c ON c.id = chunks_fts.rowid WHERE chunks_fts MATCH ?))");
                args.push(Box::new(q.clone()));
                args.push(Box::new(q));
            }
            None => return Ok(vec![]),
        }
    }
    if let Some(t) = f.tag.as_ref().filter(|t| !t.is_empty()) {
        sql.push_str(" AND id IN (SELECT paper_id FROM paper_tags WHERE tag = ?)");
        args.push(Box::new(t.clone()));
    }
    if let Some(p) = f.project_id.as_ref().filter(|t| !t.is_empty()) {
        sql.push_str(" AND id IN (SELECT paper_id FROM project_papers WHERE project_id = ?)");
        args.push(Box::new(p.clone()));
    }
    if let Some(s) = f.status.as_ref().filter(|t| !t.is_empty()) {
        sql.push_str(" AND text_status = ?");
        args.push(Box::new(s.clone()));
    }
    sql.push_str(match f.sort.as_deref() {
        Some("title") => " ORDER BY title COLLATE NOCASE",
        Some("year") => " ORDER BY year DESC NULLS LAST, title",
        _ => " ORDER BY created_at DESC, title",
    });
    let mut st = conn.prepare(&sql)?;
    let rows = st.query_map(rusqlite::params_from_iter(args.iter().map(|b| b.as_ref())), row_to_paper)?;
    let mut out = Vec::new();
    for r in rows {
        let mut p = r?;
        load_tags(conn, &mut p)?;
        out.push(p);
    }
    Ok(out)
}

pub fn all_tags(conn: &Connection) -> AppResult<Vec<(String, i64)>> {
    let mut st = conn.prepare("SELECT tag, count(*) FROM paper_tags GROUP BY tag ORDER BY tag")?;
    let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    Ok(v)
}

fn refresh_paper_fts(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM papers_fts WHERE paper_id=?1", [id])?;
    let p = get_paper(conn, id)?;
    let s = format!("{} {} {} {} {} {} {}", p.title, p.authors.join(" "), p.journal, p.abstract_text, p.keywords.join(" "), p.field, p.tags.join(" "));
    conn.execute("INSERT INTO papers_fts(paper_id, tokens) VALUES (?1, ?2)", params![id, text::index_string(&s)])?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetadataUpdate {
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub journal: String,
    pub doi: String,
    #[serde(rename = "abstract")]
    pub abstract_text: String,
    pub keywords: Vec<String>,
    pub field: String,
    pub memo: String,
    pub tags: Vec<String>,
}

/// 사용자가 수정한 서지정보 저장. 바뀐 필드의 출처는 `user`가 된다.
pub fn update_metadata(conn: &mut Connection, id: &str, u: &MetadataUpdate) -> AppResult<Paper> {
    let old = get_paper(conn, id)?;
    let mut src = old.meta_source.clone();
    let mut mark = |k: &str, changed: bool| {
        if changed {
            src.insert(k.into(), "user".into());
        }
    };
    mark("title", old.title != u.title);
    mark("authors", old.authors != u.authors);
    mark("year", old.year != u.year);
    mark("journal", old.journal != u.journal);
    mark("doi", old.doi != u.doi);
    mark("abstract", old.abstract_text != u.abstract_text);
    mark("keywords", old.keywords != u.keywords);
    let clean = |v: &[String]| -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for x in v.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
            if !out.contains(&x) {
                out.push(x);
            }
        }
        out
    };
    if u.title.trim().is_empty() {
        return Err(AppError::Invalid("제목은 비워 둘 수 없습니다".into()));
    }
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE papers SET title=?2, authors=?3, year=?4, journal=?5, doi=?6, abstract=?7, keywords=?8, field=?9, memo=?10, meta_source=?11, updated_at=?12 WHERE id=?1",
        params![
            id,
            u.title.trim(),
            serde_json::to_string(&clean(&u.authors))?,
            u.year,
            u.journal.trim(),
            u.doi.trim(),
            u.abstract_text.trim(),
            serde_json::to_string(&clean(&u.keywords))?,
            u.field.trim(),
            u.memo,
            serde_json::to_string(&src)?,
            crate::now()
        ],
    )?;
    tx.execute("DELETE FROM paper_tags WHERE paper_id=?1", [id])?;
    for t in clean(&u.tags) {
        tx.execute("INSERT OR IGNORE INTO paper_tags(paper_id, tag) VALUES (?1, ?2)", params![id, t])?;
    }
    refresh_paper_fts(&tx, id)?;
    tx.commit()?;
    get_paper(conn, id)
}

pub fn set_tags(conn: &mut Connection, ids: &[String], add: &[String], remove: &[String]) -> AppResult<()> {
    let tx = conn.transaction()?;
    for id in ids {
        for t in add.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            tx.execute("INSERT OR IGNORE INTO paper_tags(paper_id, tag) VALUES (?1, ?2)", params![id, t])?;
        }
        for t in remove {
            tx.execute("DELETE FROM paper_tags WHERE paper_id=?1 AND tag=?2", params![id, t])?;
        }
        refresh_paper_fts(&tx, id)?;
    }
    tx.commit()?;
    Ok(())
}

/// 연구실에서 논문을 삭제한다 (교수님의 원본 파일은 삭제하지 않음).
pub fn delete_paper(conn: &mut Connection, paths: &LibraryPaths, id: &str) -> AppResult<()> {
    let p = get_paper(conn, id)?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM chunks_fts WHERE rowid IN (SELECT id FROM chunks WHERE paper_id=?1)", [id])?;
    tx.execute("DELETE FROM papers_fts WHERE paper_id=?1", [id])?;
    tx.execute("DELETE FROM links WHERE (src_type='paper' AND src_id=?1) OR (dst_type='paper' AND dst_id=?1)", [id])?;
    tx.execute("DELETE FROM papers WHERE id=?1", [id])?;
    tx.commit()?;
    if let Some(rel) = p.file_path {
        let abs = paths.resolve_stored(&rel)?;
        // 연구실 papers/ 폴더 안의 사본만 지운다
        if abs.starts_with(paths.papers()) {
            let _ = std::fs::remove_file(abs);
        }
    }
    Ok(())
}

pub fn sha256_file(path: &Path) -> AppResult<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// 폴더 안의 등록 가능한 파일을 찾는다 (하위 폴더 포함, 숨김 폴더 제외).
pub fn collect_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .max_depth(12)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| {
            let x = extract::ext_of(p);
            extract::document_extensions().contains(&x.as_str()) || x == "bib" || x == "ris"
        })
        .collect();
    v.sort();
    v
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStatus {
    Added,
    Duplicate,
    Failed,
    Updated,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportOutcome {
    pub file_name: String,
    pub source_path: String,
    pub status: ImportStatus,
    pub paper_id: Option<String>,
    pub message: String,
    pub warnings: Vec<String>,
}

/// DB 잠금 없이 할 수 있는 준비 작업 (해시 계산, 본문 추출, 서지정보 추출)
pub struct Prepared {
    pub source: PathBuf,
    pub file_name: String,
    pub hash: String,
    pub size: u64,
    pub ext: String,
    pub extracted: Result<Extracted, String>,
    pub meta: Metadata,
}

pub fn find_by_hash(conn: &Connection, hash: &str) -> AppResult<Option<(String, String)>> {
    Ok(conn.query_row("SELECT id, title FROM papers WHERE file_hash=?1", [hash], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
}

pub fn hash_and_check(path: &Path) -> AppResult<(String, u64)> {
    let meta = std::fs::metadata(path).map_err(|_| AppError::File("파일을 찾을 수 없습니다".into()))?;
    if !meta.is_file() {
        return Err(AppError::File("파일이 아닙니다".into()));
    }
    if meta.len() > extract::MAX_FILE_BYTES {
        return Err(AppError::File("파일이 너무 큽니다 (최대 300MB)".into()));
    }
    let ext = extract::ext_of(path);
    if !extract::document_extensions().contains(&ext.as_str()) {
        return Err(AppError::Unsupported(format!("지원하지 않는 파일 형식입니다 (.{ext})")));
    }
    Ok((sha256_file(path)?, meta.len()))
}

pub fn prepare(path: &Path, hash: String, size: u64) -> Prepared {
    let file_name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = extract::ext_of(path);
    let extracted = extract::extract(path).map_err(|e| e.to_string());
    let meta = match &extracted {
        Ok(e) => metadata::extract_metadata(e, &file_name),
        Err(_) => {
            let mut m = Metadata { title: Path::new(&file_name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), ..Default::default() };
            m.source.insert("title".into(), "file_name".into());
            m
        }
    };
    Prepared { source: path.to_path_buf(), file_name, hash, size, ext, extracted, meta }
}

fn insert_chunks(conn: &Connection, paper_id: &str, e: &Extracted) -> AppResult<usize> {
    let mut ord = 0i64;
    let mut ins = conn.prepare_cached("INSERT INTO chunks(paper_id, page, ord, text) VALUES (?1, ?2, ?3, ?4)")?;
    let mut fts = conn.prepare_cached("INSERT INTO chunks_fts(rowid, tokens) VALUES (?1, ?2)")?;
    for (page, t) in &e.pages {
        for c in text::chunk_page(t, CHUNK_TARGET, CHUNK_MAX) {
            ins.execute(params![paper_id, page, ord, c])?;
            let rowid = conn.last_insert_rowid();
            fts.execute(params![rowid, text::index_string(&c)])?;
            ord += 1;
        }
    }
    Ok(ord as usize)
}

fn clear_chunks(conn: &Connection, paper_id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM chunks_fts WHERE rowid IN (SELECT id FROM chunks WHERE paper_id=?1)", [paper_id])?;
    conn.execute("DELETE FROM chunks WHERE paper_id=?1", [paper_id])?;
    Ok(())
}

fn status_of(e: &Result<Extracted, String>) -> (String, String, Vec<String>) {
    match e {
        Ok(x) => (x.status().to_string(), x.warnings.join(" "), x.warnings.clone()),
        Err(m) => ("failed".into(), m.clone(), vec![m.clone()]),
    }
}

/// 준비된 파일을 연구실에 등록한다.
pub fn commit(conn: &mut Connection, paths: &LibraryPaths, p: Prepared) -> AppResult<ImportOutcome> {
    let base = ImportOutcome {
        file_name: p.file_name.clone(),
        source_path: p.source.to_string_lossy().into_owned(),
        status: ImportStatus::Added,
        paper_id: None,
        message: String::new(),
        warnings: vec![],
    };
    if let Some((id, title)) = find_by_hash(conn, &p.hash)? {
        return Ok(ImportOutcome { status: ImportStatus::Duplicate, paper_id: Some(id), message: format!("이미 등록된 파일입니다: {title}"), ..base });
    }
    // 사본 저장 (원본은 수정하지 않음)
    let rel = format!("papers/{}.{}", &p.hash[..20], p.ext);
    let dest = paths.resolve_stored(&rel)?;
    if !dest.exists() {
        let tmp = dest.with_extension("part");
        std::fs::copy(&p.source, &tmp)?;
        std::fs::rename(&tmp, &dest)?;
    }
    let (status, detail, warnings) = status_of(&p.extracted);
    let mut warnings = warnings;
    // DOI가 같은 논문이 이미 있으면 알림 (다른 판본일 수 있으므로 등록은 진행)
    if !p.meta.doi.is_empty() {
        if let Some(t) = conn.query_row("SELECT title FROM papers WHERE lower(doi)=lower(?1)", [&p.meta.doi], |r| r.get::<_, String>(0)).optional()? {
            warnings.push(format!("같은 DOI의 논문이 이미 있습니다: {t}"));
        }
    }
    let id = crate::new_id();
    let now = crate::now();
    let m = &p.meta;
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO papers(id,title,authors,year,journal,doi,abstract,keywords,file_path,original_name,file_hash,file_kind,file_size,page_count,text_status,status_detail,meta_source,created_at,updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?18)",
        params![
            id,
            m.title,
            serde_json::to_string(&m.authors)?,
            m.year,
            m.journal,
            m.doi,
            m.abstract_text,
            serde_json::to_string(&m.keywords)?,
            rel,
            p.file_name,
            p.hash,
            p.ext,
            p.size as i64,
            p.extracted.as_ref().ok().and_then(|e| e.page_count).map(|n| n as i64),
            status,
            detail,
            serde_json::to_string(&m.source)?,
            now
        ],
    )?;
    if let Ok(e) = &p.extracted {
        insert_chunks(&tx, &id, e)?;
    }
    refresh_paper_fts(&tx, &id)?;
    tx.commit()?;
    let _ = detect_citations(conn, &id);
    let message = match status.as_str() {
        "ok" => "등록했습니다".to_string(),
        "partial" => "등록했습니다 (일부 쪽은 글자를 찾지 못했습니다)".into(),
        "no_text" => "등록했지만 글자를 찾지 못했습니다 (OCR 필요)".into(),
        _ => format!("파일은 보관했지만 본문을 읽지 못했습니다: {detail}"),
    };
    let st = if status == "failed" { ImportStatus::Failed } else { ImportStatus::Added };
    Ok(ImportOutcome { status: st, paper_id: Some(id), message, warnings, ..base })
}

/// 한 파일 등록 (테스트·단순 호출용)
pub fn import_file(conn: &mut Connection, paths: &LibraryPaths, path: &Path) -> AppResult<ImportOutcome> {
    let (hash, size) = hash_and_check(path)?;
    if let Some((id, title)) = find_by_hash(conn, &hash)? {
        return Ok(ImportOutcome {
            file_name: path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            source_path: path.to_string_lossy().into_owned(),
            status: ImportStatus::Duplicate,
            paper_id: Some(id),
            message: format!("이미 등록된 파일입니다: {title}"),
            warnings: vec![],
        });
    }
    let p = prepare(path, hash, size);
    commit(conn, paths, p)
}

/// 저장된 사본에서 본문을 다시 추출한다 (실패한 파일 재처리). 사용자가 수정한 서지정보는 유지한다.
pub fn reprocess(conn: &mut Connection, paths: &LibraryPaths, id: &str) -> AppResult<Paper> {
    let p = get_paper(conn, id)?;
    let rel = p.file_path.clone().ok_or_else(|| AppError::Invalid("연결된 파일이 없는 서지 항목입니다".into()))?;
    let abs = paths.resolve_stored(&rel)?;
    let extracted = extract::extract(&abs).map_err(|e| e.to_string());
    let (status, detail, _) = status_of(&extracted);
    let tx = conn.transaction()?;
    clear_chunks(&tx, id)?;
    tx.execute("DELETE FROM embeddings WHERE chunk_id IN (SELECT id FROM chunks WHERE paper_id=?1)", [id])?;
    if let Ok(e) = &extracted {
        insert_chunks(&tx, id, e)?;
        // 사용자가 수정하지 않았고 비어 있는 서지정보만 채운다
        let m = metadata::extract_metadata(e, &p.original_name);
        let mut src = p.meta_source.clone();
        if p.abstract_text.is_empty() && !m.abstract_text.is_empty() {
            tx.execute("UPDATE papers SET abstract=?2 WHERE id=?1", params![id, m.abstract_text])?;
            src.insert("abstract".into(), "text".into());
        }
        if p.doi.is_empty() && !m.doi.is_empty() {
            tx.execute("UPDATE papers SET doi=?2 WHERE id=?1", params![id, m.doi])?;
            src.insert("doi".into(), "text".into());
        }
        tx.execute("UPDATE papers SET meta_source=?2, page_count=?3 WHERE id=?1", params![id, serde_json::to_string(&src)?, e.page_count.map(|n| n as i64)])?;
    }
    tx.execute("UPDATE papers SET text_status=?2, status_detail=?3, updated_at=?4 WHERE id=?1", params![id, status, detail, crate::now()])?;
    refresh_paper_fts(&tx, id)?;
    tx.commit()?;
    let _ = detect_citations(conn, id);
    get_paper(conn, id)
}

/// 서지 파일(BibTeX/RIS) 항목을 연구실에 반영한다.
/// - DOI 또는 제목이 같은 논문이 있으면 비어 있는 서지정보만 채운다 (서지정보 연결)
/// - 없으면 서지 항목(본문 없음)을 새로 만든다
/// - 항목에 PDF 경로가 있고 그 파일이 실제로 있으면 함께 등록한다
pub fn import_bib_entries(conn: &mut Connection, paths: &LibraryPaths, entries: &[BibEntry], source_kind: &str) -> AppResult<Vec<ImportOutcome>> {
    let mut out = Vec::new();
    for e in entries {
        let label = if e.title.is_empty() { e.key.clone() } else { e.title.clone() };
        let base = ImportOutcome { file_name: label.clone(), source_path: String::new(), status: ImportStatus::Added, paper_id: None, message: String::new(), warnings: vec![] };
        if e.title.trim().is_empty() {
            out.push(ImportOutcome { status: ImportStatus::Failed, message: "제목이 없는 항목은 건너뛰었습니다".into(), ..base });
            continue;
        }
        // 첨부 PDF가 실제로 존재하면 먼저 등록
        let mut attached: Option<String> = None;
        let mut warnings = vec![];
        for f in &e.files {
            let fp = PathBuf::from(f);
            if fp.is_file() {
                match import_file(conn, paths, &fp) {
                    Ok(o) => attached = o.paper_id,
                    Err(err) => warnings.push(format!("첨부 파일을 등록하지 못했습니다: {err}")),
                }
                break;
            }
        }
        let existing = match attached.clone() {
            Some(id) => Some(id),
            None => find_existing(conn, e)?,
        };
        match existing {
            Some(id) => {
                merge_bib(conn, &id, e, source_kind)?;
                out.push(ImportOutcome { status: ImportStatus::Updated, paper_id: Some(id), message: if attached.is_some() { "PDF와 서지정보를 등록했습니다".into() } else { "기존 논문에 서지정보를 연결했습니다".into() }, warnings, ..base });
            }
            None => {
                let id = crate::new_id();
                let now = crate::now();
                let mut src = BTreeMap::new();
                for k in ["title", "authors", "year", "journal", "doi", "abstract", "keywords"] {
                    src.insert(k.to_string(), source_kind.to_string());
                }
                let tx = conn.transaction()?;
                tx.execute(
                    "INSERT INTO papers(id,title,authors,year,journal,doi,abstract,keywords,file_kind,text_status,status_detail,meta_source,bib_key,bib_type,created_at,updated_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'metadata_only','서지정보만 있습니다. PDF를 등록하면 본문 검색이 가능합니다.',?10,?11,?12,?13,?13)",
                    params![id, e.title, serde_json::to_string(&e.authors)?, e.year, e.journal, e.doi, e.abstract_text, serde_json::to_string(&e.keywords)?, source_kind, serde_json::to_string(&src)?, e.key, e.entry_type, now],
                )?;
                refresh_paper_fts(&tx, &id)?;
                tx.commit()?;
                if !e.files.is_empty() && attached.is_none() {
                    warnings.push("서지 파일에 적힌 PDF 경로를 찾을 수 없어 서지정보만 등록했습니다.".into());
                }
                out.push(ImportOutcome { status: ImportStatus::Added, paper_id: Some(id), message: "서지정보를 등록했습니다".into(), warnings, ..base });
            }
        }
    }
    Ok(out)
}

fn find_existing(conn: &Connection, e: &BibEntry) -> AppResult<Option<String>> {
    if !e.doi.is_empty() {
        if let Some(id) = conn.query_row("SELECT id FROM papers WHERE lower(doi)=lower(?1)", [&e.doi], |r| r.get(0)).optional()? {
            return Ok(Some(id));
        }
    }
    let nt = text::norm_title(&e.title);
    if nt.chars().count() >= 8 {
        let mut st = conn.prepare("SELECT id, title FROM papers")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for r in rows {
            let (id, t) = r?;
            if text::norm_title(&t) == nt {
                return Ok(Some(id));
            }
        }
    }
    Ok(None)
}

/// 서지 파일의 값으로 기존 논문의 서지정보를 보완한다. 사용자가 직접 입력한 값은 덮어쓰지 않는다.
/// 본문에서 추정한 값(text_guess, file_name)은 서지 파일 값으로 교체한다.
fn merge_bib(conn: &mut Connection, id: &str, e: &BibEntry, kind: &str) -> AppResult<()> {
    let p = get_paper(conn, id)?;
    let mut src = p.meta_source.clone();
    let replaceable = |field: &str, empty: bool| -> bool {
        let s = p.meta_source.get(field).map(String::as_str).unwrap_or("");
        s != "user" && (empty || s == "text_guess" || s == "file_name" || s == "pdf_info")
    };
    let tx = conn.transaction()?;
    let mut set = |col: &str, field: &str, val: rusqlite::types::Value, cur_empty: bool, new_empty: bool| -> AppResult<()> {
        if !new_empty && replaceable(field, cur_empty) {
            tx.execute(&format!("UPDATE papers SET {col}=?2 WHERE id=?1"), params![id, val])?;
            src.insert(field.into(), kind.into());
        }
        Ok(())
    };
    use rusqlite::types::Value as V;
    set("title", "title", V::Text(e.title.clone()), p.title.is_empty(), e.title.is_empty())?;
    set("authors", "authors", V::Text(serde_json::to_string(&e.authors)?), p.authors.is_empty(), e.authors.is_empty())?;
    set("year", "year", e.year.map(|y| V::Integer(y as i64)).unwrap_or(V::Null), p.year.is_none(), e.year.is_none())?;
    set("journal", "journal", V::Text(e.journal.clone()), p.journal.is_empty(), e.journal.is_empty())?;
    set("doi", "doi", V::Text(e.doi.clone()), p.doi.is_empty(), e.doi.is_empty())?;
    set("abstract", "abstract", V::Text(e.abstract_text.clone()), p.abstract_text.is_empty(), e.abstract_text.is_empty())?;
    set("keywords", "keywords", V::Text(serde_json::to_string(&e.keywords)?), p.keywords.is_empty(), e.keywords.is_empty())?;
    tx.execute(
        "UPDATE papers SET meta_source=?2, bib_key=CASE WHEN bib_key='' THEN ?3 ELSE bib_key END, bib_type=CASE WHEN bib_type='' THEN ?4 ELSE bib_type END, updated_at=?5 WHERE id=?1",
        params![id, serde_json::to_string(&src)?, e.key, e.entry_type, crate::now()],
    )?;
    refresh_paper_fts(&tx, id)?;
    tx.commit()?;
    Ok(())
}

/// 등록된 논문들 사이의 실제 인용 관계를 찾는다.
/// 근거: 한 논문의 본문(주로 참고문헌)에 다른 논문의 DOI 또는 충분히 긴 제목이 그대로 나타남.
pub fn detect_citations(conn: &Connection, paper_id: &str) -> AppResult<usize> {
    let papers: Vec<(String, String, String)> = {
        let mut st = conn.prepare("SELECT id, title, doi FROM papers")?;
        let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
        v
    };
    let chunks_of = |pid: &str| -> AppResult<Vec<(Option<i64>, String)>> {
        let mut st = conn.prepare_cached("SELECT page, text FROM chunks WHERE paper_id=?1 ORDER BY ord")?;
        let v = st.query_map([pid], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        Ok(v)
    };
    let find = |chunks: &[(Option<i64>, String)], title: &str, doi: &str| -> Option<(String, Option<i64>)> {
        let doi_l = doi.to_lowercase();
        let nt = text::norm_title(title);
        for (page, t) in chunks {
            if !doi_l.is_empty() && t.to_lowercase().contains(&doi_l) {
                return Some(("doi".into(), *page));
            }
            if nt.chars().count() >= 20 && text::norm_title(t).contains(&nt) {
                return Some(("title".into(), *page));
            }
        }
        None
    };
    let mut n = 0;
    let me = papers.iter().find(|p| p.0 == paper_id).cloned();
    let Some(me) = me else { return Ok(0) };
    let my_chunks = chunks_of(paper_id)?;
    for other in papers.iter().filter(|p| p.0 != paper_id) {
        // 내가 다른 논문을 인용
        if let Some((ev, page)) = find(&my_chunks, &other.1, &other.2) {
            conn.execute("INSERT OR REPLACE INTO citations(citing_id, cited_id, evidence, page) VALUES (?1,?2,?3,?4)", params![paper_id, other.0, ev, page])?;
            n += 1;
        }
        // 다른 논문이 나를 인용
        let oc = chunks_of(&other.0)?;
        if let Some((ev, page)) = find(&oc, &me.1, &me.2) {
            conn.execute("INSERT OR REPLACE INTO citations(citing_id, cited_id, evidence, page) VALUES (?1,?2,?3,?4)", params![other.0, paper_id, ev, page])?;
            n += 1;
        }
    }
    Ok(n)
}

#[derive(Debug, Clone, Serialize)]
pub struct ChunkRow {
    pub id: i64,
    pub paper_id: String,
    pub page: Option<i64>,
    pub ord: i64,
    pub text: String,
}

pub fn paper_chunks(conn: &Connection, paper_id: &str) -> AppResult<Vec<ChunkRow>> {
    let mut st = conn.prepare("SELECT id, paper_id, page, ord, text FROM chunks WHERE paper_id=?1 ORDER BY ord")?;
    let v = st
        .query_map([paper_id], |r| Ok(ChunkRow { id: r.get(0)?, paper_id: r.get(1)?, page: r.get(2)?, ord: r.get(3)?, text: r.get(4)? }))?
        .collect::<Result<_, _>>()?;
    Ok(v)
}

pub fn paper_file_abs(conn: &Connection, paths: &LibraryPaths, id: &str) -> AppResult<PathBuf> {
    let rel: Option<String> = conn.query_row("SELECT file_path FROM papers WHERE id=?1", [id], |r| r.get(0)).map_err(|_| AppError::NotFound("논문을 찾을 수 없습니다".into()))?;
    let rel = rel.ok_or_else(|| AppError::NotFound("이 항목에는 연결된 파일이 없습니다".into()))?;
    let abs = paths.resolve_stored(&rel)?;
    if !abs.is_file() {
        return Err(AppError::NotFound("연구실 폴더에서 파일을 찾을 수 없습니다. 백업에서 복원하거나 다시 등록해 주세요.".into()));
    }
    Ok(abs)
}

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub papers: i64,
    pub papers_with_text: i64,
    pub notes: i64,
    pub projects_active: i64,
    pub chunks: i64,
    pub embedded_chunks: i64,
    pub comparisons: i64,
    pub needs_attention: i64,
}

pub fn stats(conn: &Connection) -> AppResult<Stats> {
    let q = |sql: &str| -> AppResult<i64> { Ok(conn.query_row(sql, [], |r| r.get(0))?) };
    Ok(Stats {
        papers: q("SELECT count(*) FROM papers")?,
        papers_with_text: q("SELECT count(*) FROM papers WHERE text_status IN ('ok','partial')")?,
        notes: q("SELECT count(*) FROM notes")?,
        projects_active: q("SELECT count(*) FROM projects WHERE status='active'")?,
        chunks: q("SELECT count(*) FROM chunks")?,
        embedded_chunks: q("SELECT count(*) FROM embeddings")?,
        comparisons: q("SELECT count(*) FROM comparisons")?,
        needs_attention: q("SELECT count(*) FROM papers WHERE text_status IN ('failed','no_text')")?,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::extract::tests::make_pdf;

    pub fn lib() -> (tempfile::TempDir, LibraryPaths, Connection) {
        let d = tempfile::tempdir().unwrap();
        let p = LibraryPaths::new(d.path().join("lib"));
        p.ensure(false).unwrap();
        let (c, _) = crate::db::open(&p).unwrap();
        (d, p, c)
    }

    pub fn write_pdf(dir: &Path, name: &str, pages: &[&str], title: Option<&str>) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, make_pdf(pages, title)).unwrap();
        path
    }

    #[test]
    fn import_dedupe_search_delete_keeps_original() {
        let (d, paths, mut c) = lib();
        let src = write_pdf(d.path(), "a.pdf", &["Generative AI and Critical Thinking in Writing Courses", "Keywords: critical thinking, writing\nWe surveyed 120 students about reflective judgment."], Some("Generative AI and Critical Thinking in Writing Courses"));
        let before = std::fs::read(&src).unwrap();
        let o = import_file(&mut c, &paths, &src).unwrap();
        assert!(matches!(o.status, ImportStatus::Added));
        let id = o.paper_id.unwrap();
        let p = get_paper(&c, &id).unwrap();
        assert_eq!(p.page_count, Some(2));
        assert_eq!(p.text_status, "ok");
        assert_eq!(p.meta_source["title"], "pdf_info");
        assert!(p.chunk_count >= 2);
        // 중복
        let o2 = import_file(&mut c, &paths, &src).unwrap();
        assert!(matches!(o2.status, ImportStatus::Duplicate));
        // 검색 (본문)
        let r = list_papers(&c, &PaperFilter { query: "reflective judgment".into(), ..Default::default() }).unwrap();
        assert_eq!(r.len(), 1);
        let r = list_papers(&c, &PaperFilter { query: "quantum".into(), ..Default::default() }).unwrap();
        assert!(r.is_empty());
        // 수정
        let u = MetadataUpdate { title: "수정된 제목".into(), authors: vec!["홍길동".into()], year: Some(2024), journal: "교육학연구".into(), doi: "".into(), abstract_text: "".into(), keywords: vec!["AI".into()], field: "교육학".into(), memo: "메모".into(), tags: vec!["핵심".into(), "핵심".into()] };
        let p = update_metadata(&mut c, &id, &u).unwrap();
        assert_eq!(p.meta_source["title"], "user");
        assert_eq!(p.tags, vec!["핵심"]);
        let r = list_papers(&c, &PaperFilter { query: "수정된".into(), ..Default::default() }).unwrap();
        assert_eq!(r.len(), 1);
        let copy = paper_file_abs(&c, &paths, &id).unwrap();
        assert!(copy.exists());
        // 삭제해도 원본은 그대로
        delete_paper(&mut c, &paths, &id).unwrap();
        assert!(!copy.exists());
        assert_eq!(std::fs::read(&src).unwrap(), before);
        assert_eq!(stats(&c).unwrap().papers, 0);
    }

    #[test]
    fn failed_file_can_be_reprocessed() {
        let (d, paths, mut c) = lib();
        let src = d.path().join("broken.pdf");
        std::fs::write(&src, b"%PDF-1.4 broken").unwrap();
        let o = import_file(&mut c, &paths, &src).unwrap();
        assert!(matches!(o.status, ImportStatus::Failed));
        let id = o.paper_id.unwrap();
        assert_eq!(get_paper(&c, &id).unwrap().text_status, "failed");
        // 사본을 정상 파일로 바꿔 재처리 시뮬레이션
        let copy = paper_file_abs(&c, &paths, &id).unwrap();
        std::fs::write(&copy, make_pdf(&["Now the text is readable and long enough to index."], None)).unwrap();
        let p = reprocess(&mut c, &paths, &id).unwrap();
        assert_eq!(p.text_status, "ok");
        assert!(p.chunk_count > 0);
    }

    #[test]
    fn bib_links_to_existing_and_creates_metadata_only() {
        let (d, paths, mut c) = lib();
        let src = write_pdf(d.path(), "x.pdf", &["Some Unclear Heading Line Here\nbody text body text body text"], None);
        let id = import_file(&mut c, &paths, &src).unwrap().paper_id.unwrap();
        let guessed = get_paper(&c, &id).unwrap();
        assert_eq!(guessed.meta_source["title"], "text_guess");
        let entries = crate::bib::parse_bibtex("@article{a, title={Some Unclear Heading Line Here}, author={Kim, A.}, year={2021}, doi={10.1/abc}}\n@article{b, title={Entirely Different Paper}, year={2019}}");
        let out = import_bib_entries(&mut c, &paths, &entries, "bibtex").unwrap();
        assert!(matches!(out[0].status, ImportStatus::Updated));
        assert!(matches!(out[1].status, ImportStatus::Added));
        let p = get_paper(&c, &id).unwrap();
        assert_eq!(p.authors, vec!["Kim, A."]);
        assert_eq!(p.year, Some(2021));
        assert_eq!(p.meta_source["authors"], "bibtex");
        let m = get_paper(&c, out[1].paper_id.as_ref().unwrap()).unwrap();
        assert_eq!(m.text_status, "metadata_only");
        // 같은 서지 파일을 다시 가져와도 중복 생성되지 않는다
        let again = import_bib_entries(&mut c, &paths, &entries, "bibtex").unwrap();
        assert!(again.iter().all(|o| matches!(o.status, ImportStatus::Updated)));
        assert_eq!(stats(&c).unwrap().papers, 2);
    }

    #[test]
    fn detects_real_citation_by_title() {
        let (d, paths, mut c) = lib();
        let a = write_pdf(d.path(), "a.pdf", &["Foundations of Reflective Judgment in Higher Education\nThis is the cited paper body."], Some("Foundations of Reflective Judgment in Higher Education"));
        let b = write_pdf(d.path(), "b.pdf", &["A later study on tutoring", "References\nPark, J. (2019). Foundations of Reflective Judgment in Higher Education. Journal."], Some("A later study on AI tutoring"));
        let ida = import_file(&mut c, &paths, &a).unwrap().paper_id.unwrap();
        let idb = import_file(&mut c, &paths, &b).unwrap().paper_id.unwrap();
        let (citing, cited, ev, page): (String, String, String, Option<i64>) = c.query_row("SELECT citing_id, cited_id, evidence, page FROM citations", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap();
        assert_eq!((citing.as_str(), cited.as_str(), ev.as_str(), page), (idb.as_str(), ida.as_str(), "title", Some(2)));
    }

    #[test]
    fn collects_supported_files_only() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("sub/.hidden")).unwrap();
        std::fs::write(d.path().join("a.pdf"), b"x").unwrap();
        std::fs::write(d.path().join("sub/b.docx"), b"x").unwrap();
        std::fs::write(d.path().join("sub/c.exe"), b"x").unwrap();
        std::fs::write(d.path().join("sub/.hidden/d.pdf"), b"x").unwrap();
        std::fs::write(d.path().join("refs.bib"), b"x").unwrap();
        let f = collect_files(d.path());
        assert_eq!(f.len(), 3);
    }
}
