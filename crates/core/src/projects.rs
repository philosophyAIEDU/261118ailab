//! 연구 프로젝트: 논문·노트를 주제별로 묶는다.
use crate::error::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub paper_ids: Vec<String>,
    pub note_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub status: Option<String>,
}

fn load(conn: &Connection, id: &str) -> AppResult<Project> {
    let mut p = conn
        .query_row("SELECT id,name,description,status,created_at,updated_at,(SELECT count(*) FROM notes WHERE project_id=projects.id) FROM projects WHERE id=?1", [id], |r| {
            Ok(Project { id: r.get(0)?, name: r.get(1)?, description: r.get(2)?, status: r.get(3)?, created_at: r.get(4)?, updated_at: r.get(5)?, note_count: r.get(6)?, paper_ids: vec![] })
        })
        .map_err(|_| AppError::NotFound("프로젝트를 찾을 수 없습니다".into()))?;
    let mut st = conn.prepare_cached("SELECT paper_id FROM project_papers WHERE project_id=?1")?;
    p.paper_ids = st.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    Ok(p)
}

fn status_ok(s: &str) -> &str {
    match s {
        "paused" | "done" => s,
        _ => "active",
    }
}

pub fn create(conn: &Connection, i: &ProjectInput) -> AppResult<Project> {
    if i.name.trim().is_empty() {
        return Err(AppError::Invalid("프로젝트 이름을 입력해 주세요".into()));
    }
    let id = crate::new_id();
    let now = crate::now();
    conn.execute(
        "INSERT INTO projects(id,name,description,status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?5)",
        params![id, i.name.trim(), i.description, status_ok(i.status.as_deref().unwrap_or("active")), now],
    )?;
    load(conn, &id)
}

pub fn update(conn: &Connection, id: &str, i: &ProjectInput) -> AppResult<Project> {
    if i.name.trim().is_empty() {
        return Err(AppError::Invalid("프로젝트 이름을 입력해 주세요".into()));
    }
    conn.execute(
        "UPDATE projects SET name=?2, description=?3, status=?4, updated_at=?5 WHERE id=?1",
        params![id, i.name.trim(), i.description, status_ok(i.status.as_deref().unwrap_or("active")), crate::now()],
    )?;
    load(conn, id)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM projects WHERE id=?1", [id])?;
    Ok(())
}

pub fn list(conn: &Connection) -> AppResult<Vec<Project>> {
    let mut st = conn.prepare("SELECT id FROM projects ORDER BY CASE status WHEN 'active' THEN 0 WHEN 'paused' THEN 1 ELSE 2 END, updated_at DESC")?;
    let ids: Vec<String> = st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
    ids.iter().map(|i| load(conn, i)).collect()
}

pub fn set_papers(conn: &Connection, id: &str, add: &[String], remove: &[String]) -> AppResult<Project> {
    for p in add {
        conn.execute("INSERT OR IGNORE INTO project_papers(project_id, paper_id) SELECT ?1, id FROM papers WHERE id=?2", params![id, p])?;
    }
    for p in remove {
        conn.execute("DELETE FROM project_papers WHERE project_id=?1 AND paper_id=?2", params![id, p])?;
    }
    conn.execute("UPDATE projects SET updated_at=?2 WHERE id=?1", params![id, crate::now()])?;
    load(conn, id)
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Project> {
    load(conn, id)
}
