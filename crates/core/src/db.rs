//! SQLite 데이터베이스 열기와 스키마 마이그레이션.
//!
//! - 스키마 버전은 `PRAGMA user_version`으로 관리한다.
//! - 기존 데이터가 있는 DB를 새 버전으로 올리기 전에는 반드시 `backups/`에 사본을 만든다.
//! - 마이그레이션은 하나의 트랜잭션에서 실행되어 실패하면 원래 상태로 되돌아간다.
//! - 앱보다 새로운 스키마의 DB는 열지 않는다 (구버전 앱이 데이터를 손상시키지 않도록).
use crate::error::{AppError, AppResult};
use crate::paths::LibraryPaths;
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};

/// 각 원소가 한 버전의 마이그레이션. 인덱스 + 1 = 도달하는 버전.
/// **이미 배포된 마이그레이션은 절대 수정하지 말고 새 항목을 추가한다.**
const MIGRATIONS: &[&str] = &[
    // v1 — 최초 스키마
    r#"
    CREATE TABLE settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    CREATE TABLE papers (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL DEFAULT '',
        authors TEXT NOT NULL DEFAULT '[]',        -- JSON 배열
        year INTEGER,
        journal TEXT NOT NULL DEFAULT '',
        doi TEXT NOT NULL DEFAULT '',
        abstract TEXT NOT NULL DEFAULT '',
        keywords TEXT NOT NULL DEFAULT '[]',       -- JSON 배열
        field TEXT NOT NULL DEFAULT '',
        memo TEXT NOT NULL DEFAULT '',
        file_path TEXT,                            -- 연구실 폴더 기준 상대 경로 (papers/...)
        original_name TEXT NOT NULL DEFAULT '',
        file_hash TEXT,                            -- SHA-256
        file_kind TEXT NOT NULL DEFAULT '',        -- pdf/docx/txt/md/bib
        file_size INTEGER NOT NULL DEFAULT 0,
        page_count INTEGER,
        text_status TEXT NOT NULL DEFAULT 'pending', -- pending/ok/partial/no_text/failed/metadata_only
        status_detail TEXT NOT NULL DEFAULT '',
        meta_source TEXT NOT NULL DEFAULT '{}',    -- JSON: 필드별 출처 (pdf_info/text/user/bibtex/ris)
        bib_key TEXT NOT NULL DEFAULT '',
        bib_type TEXT NOT NULL DEFAULT '',
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE UNIQUE INDEX papers_hash ON papers(file_hash) WHERE file_hash IS NOT NULL;
    CREATE INDEX papers_doi ON papers(doi);

    CREATE TABLE paper_tags (
        paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
        tag TEXT NOT NULL,
        PRIMARY KEY (paper_id, tag)
    );

    -- 검색 단위(문단 묶음). page는 PDF 쪽 번호(1부터), 쪽 정보가 없으면 NULL
    CREATE TABLE chunks (
        id INTEGER PRIMARY KEY,
        paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
        page INTEGER,
        ord INTEGER NOT NULL,
        text TEXT NOT NULL
    );
    CREATE INDEX chunks_paper ON chunks(paper_id, ord);

    -- 한국어 검색을 위해 한글은 2글자 단위(bigram)로 미리 분해한 토큰을 색인한다 (text.rs)
    CREATE VIRTUAL TABLE chunks_fts USING fts5(tokens, tokenize = 'unicode61 remove_diacritics 2');
    CREATE VIRTUAL TABLE papers_fts USING fts5(paper_id UNINDEXED, tokens, tokenize = 'unicode61 remove_diacritics 2');

    CREATE TABLE embeddings (
        chunk_id INTEGER PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
        model TEXT NOT NULL,
        dim INTEGER NOT NULL,
        vec BLOB NOT NULL
    );
    CREATE INDEX embeddings_model ON embeddings(model);

    CREATE TABLE annotations (
        id TEXT PRIMARY KEY,
        paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
        page INTEGER NOT NULL,
        kind TEXT NOT NULL,           -- highlight/memo/bookmark
        text TEXT NOT NULL DEFAULT '',
        note TEXT NOT NULL DEFAULT '',
        color TEXT NOT NULL DEFAULT '',
        rects TEXT NOT NULL DEFAULT '[]',
        created_at TEXT NOT NULL
    );
    CREATE INDEX annotations_paper ON annotations(paper_id, page);

    CREATE TABLE projects (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        status TEXT NOT NULL DEFAULT 'active',  -- active/paused/done
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE TABLE project_papers (
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
        PRIMARY KEY (project_id, paper_id)
    );

    CREATE TABLE notes (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        body TEXT NOT NULL DEFAULT '',
        project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
        origin TEXT NOT NULL DEFAULT 'user',   -- user / ai_reviewed (AI 초안을 교수님이 검토 후 저장)
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE TABLE note_papers (
        note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
        paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
        PRIMARY KEY (note_id, paper_id)
    );
    CREATE TABLE note_tags (
        note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
        tag TEXT NOT NULL,
        PRIMARY KEY (note_id, tag)
    );
    CREATE VIRTUAL TABLE notes_fts USING fts5(note_id UNINDEXED, tokens, tokenize = 'unicode61 remove_diacritics 2');

    -- 사용자/AI가 만든 지식 그래프 연결. 실제 인용 관계는 citations 표에 따로 둔다.
    CREATE TABLE links (
        id TEXT PRIMARY KEY,
        src_type TEXT NOT NULL, src_id TEXT NOT NULL,
        dst_type TEXT NOT NULL, dst_id TEXT NOT NULL,
        kind TEXT NOT NULL,           -- user / ai_suggested
        label TEXT NOT NULL DEFAULT '',
        score REAL,
        created_at TEXT NOT NULL
    );
    -- 본문/참고문헌에서 다른 등록 논문의 DOI 또는 제목이 발견된 경우 (근거 포함)
    CREATE TABLE citations (
        citing_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
        cited_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
        evidence TEXT NOT NULL,       -- doi / title
        page INTEGER,
        PRIMARY KEY (citing_id, cited_id)
    );

    CREATE TABLE searches (
        id TEXT PRIMARY KEY,
        query TEXT NOT NULL,
        mode TEXT NOT NULL,
        answer TEXT NOT NULL DEFAULT '',
        result TEXT NOT NULL DEFAULT '{}',   -- JSON (근거·검증 상태)
        model TEXT NOT NULL DEFAULT '',
        created_at TEXT NOT NULL
    );

    CREATE TABLE comparisons (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        paper_ids TEXT NOT NULL,
        aspects TEXT NOT NULL,
        result TEXT NOT NULL,
        model TEXT NOT NULL DEFAULT '',
        created_at TEXT NOT NULL
    );

    CREATE TABLE reviews (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        topic TEXT NOT NULL DEFAULT '',
        paper_ids TEXT NOT NULL,
        content TEXT NOT NULL,          -- JSON
        model TEXT NOT NULL DEFAULT '',
        created_at TEXT NOT NULL
    );
    "#,
];

pub fn latest_version() -> i64 {
    MIGRATIONS.len() as i64
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OpenReport {
    pub from_version: i64,
    pub to_version: i64,
    pub backup_path: Option<PathBuf>,
}

pub fn open(paths: &LibraryPaths) -> AppResult<(Connection, OpenReport)> {
    open_with(paths, MIGRATIONS)
}

fn configure(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA busy_timeout = 5000;",
    )?;
    Ok(())
}

fn user_version(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

pub(crate) fn open_with(paths: &LibraryPaths, migrations: &[&str]) -> AppResult<(Connection, OpenReport)> {
    let db_path = paths.db();
    let existed = db_path.exists();
    let mut conn = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    configure(&conn)?;
    let current = user_version(&conn)?;
    let target = migrations.len() as i64;
    if current > target {
        return Err(AppError::Blocked(format!(
            "이 연구실 자료는 더 새로운 버전의 프로그램에서 만들어졌습니다 (자료 형식 v{current}, 프로그램 지원 v{target}). 자료 보호를 위해 열지 않았습니다. 프로그램을 최신 버전으로 업데이트해 주세요."
        )));
    }
    let mut backup_path = None;
    if current < target {
        if existed && current > 0 {
            backup_path = Some(snapshot(&conn, &paths.backups(), &format!("pre-migration-v{current}-to-v{target}"))?);
        }
        let tx = conn.transaction()?;
        for (i, sql) in migrations.iter().enumerate().skip(current as usize) {
            tx.execute_batch(sql).map_err(|e| AppError::Db(format!("자료 형식 업그레이드(v{}) 실패: {e}", i + 1)))?;
        }
        tx.pragma_update(None, "user_version", target)?;
        tx.commit()?;
    }
    Ok((conn, OpenReport { from_version: current, to_version: target, backup_path }))
}

/// 일관된 DB 사본을 만든다 (SQLite 온라인 백업 API — 쓰는 중에도 안전).
pub fn snapshot(conn: &Connection, dir: &Path, label: &str) -> AppResult<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("research-{label}-{ts}.db"));
    let mut dst = Connection::open(&path)?;
    {
        let b = rusqlite::backup::Backup::new(conn, &mut dst)?;
        b.run_to_completion(256, std::time::Duration::from_millis(5), None)?;
    }
    Ok(path)
}

/// 데이터베이스 무결성 검사
pub fn integrity_ok(conn: &Connection) -> AppResult<bool> {
    let s: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    Ok(s == "ok")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_reopens() {
        let d = tempfile::tempdir().unwrap();
        let p = LibraryPaths::new(d.path());
        p.ensure(false).unwrap();
        let (c, r) = open(&p).unwrap();
        assert_eq!(r.from_version, 0);
        assert_eq!(r.to_version, latest_version());
        assert!(r.backup_path.is_none());
        assert!(integrity_ok(&c).unwrap());
        drop(c);
        let (_c, r) = open(&p).unwrap();
        assert_eq!(r.from_version, latest_version());
    }

    #[test]
    fn migration_backs_up_existing_data_and_keeps_it() {
        let d = tempfile::tempdir().unwrap();
        let p = LibraryPaths::new(d.path());
        p.ensure(false).unwrap();
        let v1 = [MIGRATIONS[0]];
        let (c, _) = open_with(&p, &v1).unwrap();
        c.execute("INSERT INTO settings(key,value) VALUES('k','\"v\"')", []).unwrap();
        drop(c);
        let v2 = [MIGRATIONS[0], "ALTER TABLE papers ADD COLUMN extra TEXT NOT NULL DEFAULT '';"];
        let (c, r) = open_with(&p, &v2).unwrap();
        assert_eq!((r.from_version, r.to_version), (1, 2));
        let bp = r.backup_path.unwrap();
        assert!(bp.exists());
        let v: String = c.query_row("SELECT value FROM settings WHERE key='k'", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "\"v\"");
        // 백업본에도 데이터가 있어야 한다
        let b = Connection::open(bp).unwrap();
        let n: i64 = b.query_row("SELECT count(*) FROM settings", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn failed_migration_rolls_back() {
        let d = tempfile::tempdir().unwrap();
        let p = LibraryPaths::new(d.path());
        p.ensure(false).unwrap();
        let (c, _) = open_with(&p, &[MIGRATIONS[0]]).unwrap();
        drop(c);
        let bad = [MIGRATIONS[0], "ALTER TABLE nope ADD COLUMN x;"];
        assert!(open_with(&p, &bad).is_err());
        let (c, r) = open_with(&p, &[MIGRATIONS[0]]).unwrap();
        assert_eq!(r.from_version, 1);
        drop(c);
    }

    #[test]
    fn refuses_newer_schema() {
        let d = tempfile::tempdir().unwrap();
        let p = LibraryPaths::new(d.path());
        p.ensure(false).unwrap();
        let (c, _) = open(&p).unwrap();
        c.pragma_update(None, "user_version", 999).unwrap();
        drop(c);
        assert!(matches!(open(&p), Err(AppError::Blocked(_))));
    }
}
