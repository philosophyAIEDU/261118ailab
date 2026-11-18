//! PDF 주석: 하이라이트, 메모, 북마크 (원본 PDF는 수정하지 않고 DB에만 저장).
use crate::error::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub id: String,
    pub paper_id: String,
    pub page: i64,
    pub kind: String,
    pub text: String,
    pub note: String,
    pub color: String,
    /// 쪽 크기 대비 비율 좌표 [{x,y,w,h}] (확대·축소와 무관)
    pub rects: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnnotationInput {
    pub paper_id: String,
    pub page: i64,
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub rects: serde_json::Value,
}

pub fn add(conn: &Connection, i: &AnnotationInput) -> AppResult<Annotation> {
    if !matches!(i.kind.as_str(), "highlight" | "memo" | "bookmark") {
        return Err(AppError::Invalid("주석 종류가 올바르지 않습니다".into()));
    }
    if i.page < 1 {
        return Err(AppError::Invalid("쪽 번호가 올바르지 않습니다".into()));
    }
    let id = crate::new_id();
    let rects = if i.rects.is_null() { serde_json::json!([]) } else { i.rects.clone() };
    conn.execute(
        "INSERT INTO annotations(id,paper_id,page,kind,text,note,color,rects,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![id, i.paper_id, i.page, i.kind, i.text, i.note, i.color, rects.to_string(), crate::now()],
    )?;
    Ok(list(conn, &i.paper_id)?.into_iter().find(|a| a.id == id).unwrap())
}

pub fn update_note(conn: &Connection, id: &str, note: &str) -> AppResult<()> {
    conn.execute("UPDATE annotations SET note=?2 WHERE id=?1", params![id, note])?;
    Ok(())
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM annotations WHERE id=?1", [id])?;
    Ok(())
}

pub fn list(conn: &Connection, paper_id: &str) -> AppResult<Vec<Annotation>> {
    let mut st = conn.prepare("SELECT id,paper_id,page,kind,text,note,color,rects,created_at FROM annotations WHERE paper_id=?1 ORDER BY page, created_at")?;
    let v = st
        .query_map([paper_id], |r| {
            Ok(Annotation {
                id: r.get(0)?,
                paper_id: r.get(1)?,
                page: r.get(2)?,
                kind: r.get(3)?,
                text: r.get(4)?,
                note: r.get(5)?,
                color: r.get(6)?,
                rects: serde_json::from_str(&r.get::<_, String>(7)?).unwrap_or(serde_json::json!([])),
                created_at: r.get(8)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::{lib, write_pdf};

    #[test]
    fn add_update_delete_and_cascade() {
        let (d, paths, mut c) = lib();
        let f = write_pdf(d.path(), "a.pdf", &["Annotation test page one with enough text", "Second page text for annotations"], Some("Ann"));
        let pid = crate::library::import_file(&mut c, &paths, &f).unwrap().paper_id.unwrap();
        let a = add(&c, &AnnotationInput { paper_id: pid.clone(), page: 2, kind: "highlight".into(), text: "Second page".into(), note: "".into(), color: "violet".into(), rects: serde_json::json!([{"x":0.1,"y":0.2,"w":0.3,"h":0.02}]) }).unwrap();
        assert_eq!(a.rects[0]["w"], 0.3);
        add(&c, &AnnotationInput { paper_id: pid.clone(), page: 1, kind: "bookmark".into(), text: "".into(), note: "".into(), color: "".into(), rects: serde_json::Value::Null }).unwrap();
        assert!(add(&c, &AnnotationInput { paper_id: pid.clone(), page: 0, kind: "memo".into(), text: "".into(), note: "".into(), color: "".into(), rects: serde_json::Value::Null }).is_err());
        assert!(add(&c, &AnnotationInput { paper_id: pid.clone(), page: 1, kind: "evil".into(), text: "".into(), note: "".into(), color: "".into(), rects: serde_json::Value::Null }).is_err());
        update_note(&c, &a.id, "중요").unwrap();
        let l = list(&c, &pid).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].page, 1, "쪽 순서");
        assert_eq!(l[1].note, "중요");
        delete(&c, &l[0].id).unwrap();
        crate::library::delete_paper(&mut c, &paths, &pid).unwrap();
        assert_eq!(list(&c, &pid).unwrap().len(), 0, "논문 삭제 시 주석도 삭제");
    }
}
