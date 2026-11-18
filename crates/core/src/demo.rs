//! 데모 연구실 (유튜브 시연용). **모든 자료는 가상이며 실제 논문이 아니다.**
//!
//! 데모 PDF는 별도의 데모 연구실 폴더에 등록한다 (교수님의 실제 연구실과 섞이지 않음).
//! 이 모듈은 등록된 데모 논문에 서지정보(가상)를 채우고, 연구 프로젝트 1개와 연구 노트 5개를 만든다.
use crate::error::AppResult;
use crate::notes::NoteInput;
use rusqlite::{params, Connection};
use serde::Deserialize;

#[derive(Deserialize)]
struct Spec {
    papers: Vec<SpecPaper>,
}
#[derive(Deserialize)]
struct SpecPaper {
    title: String,
    authors: Vec<String>,
    year: i32,
    journal: String,
    keywords: Vec<String>,
}

pub const SPEC_JSON: &str = include_str!("../../../demo/demo-papers.json");
pub const PROJECT_NAME: &str = "생성형 AI와 대학생의 비판적 사고 (데모)";

fn find(conn: &Connection, needle: &str) -> Option<String> {
    conn.query_row("SELECT id FROM papers WHERE title LIKE ?1", [format!("%{needle}%")], |r| r.get(0)).ok()
}

pub fn seed(conn: &mut Connection) -> AppResult<()> {
    let spec: Spec = serde_json::from_str(SPEC_JSON)?;
    // 1) 가상 서지정보 채우기 (출처: demo)
    for p in &spec.papers {
        let core = p.title.trim_start_matches("[가상 데모] ");
        if let Some(id) = find(conn, core) {
            let mut src: std::collections::BTreeMap<String, String> =
                serde_json::from_str(&conn.query_row("SELECT meta_source FROM papers WHERE id=?1", [&id], |r| r.get::<_, String>(0))?).unwrap_or_default();
            for k in ["authors", "year", "journal", "keywords"] {
                src.insert(k.into(), "demo".into());
            }
            conn.execute(
                "UPDATE papers SET authors=?2, year=?3, journal=?4, keywords=?5, field='교육학 (데모)', meta_source=?6, memo='가상 데모 자료입니다. 실제 논문이 아닙니다.' WHERE id=?1",
                params![id, serde_json::to_string(&p.authors)?, p.year, p.journal, serde_json::to_string(&p.keywords)?, serde_json::to_string(&src)?],
            )?;
            conn.execute("INSERT OR IGNORE INTO paper_tags(paper_id, tag) VALUES (?1, '데모')", [&id])?;
        }
    }
    let ids: Vec<String> = conn.prepare("SELECT id FROM papers")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
    for id in &ids {
        // 서지정보 색인 갱신
        let _ = crate::library::set_tags(conn, std::slice::from_ref(id), &[], &[]);
    }
    // 2) 프로젝트
    let project = crate::projects::create(conn, &crate::projects::ProjectInput { name: PROJECT_NAME.into(), description: "시연용 가상 연구 프로젝트: 생성형 AI 활용 수업이 대학생의 비판적 사고에 미치는 영향에 관한 문헌을 정리한다.".into(), status: Some("active".into()) })?;
    crate::projects::set_papers(conn, &project.id, &ids, &[])?;
    // 3) 연구 노트 5개
    let p01 = find(conn, "글쓰기 수업이 대학생의 비판적 사고 성향").unwrap_or_default();
    let p02 = find(conn, "의존도와 비판적 사고 기능").unwrap_or_default();
    let p03 = find(conn, "소크라테스식 질문 AI 튜터").unwrap_or_default();
    let p06 = find(conn, "AI 리터러시 교육").unwrap_or_default();
    let p12 = find(conn, "체계적 문헌고찰").unwrap_or_default();
    let notes = [
        ("[데모] 핵심 개념: 비판적 사고의 성향과 기능", "## 정리\n\n- 성향(disposition)과 기능(skill)을 구분해야 한다.\n- 데모 논문 p01은 **성향**을, p02는 **기능**을 측정했다.\n\n## 연결\n\n- [[[데모] 연구 공백 메모]]에서 측정 도구 문제를 다룬다.", vec![p01.clone(), p02.clone()], vec!["개념", "비판적 사고"]),
        ("[데모] AI를 비판 대상으로 쓰는 수업", "AI 초안을 반박하게 하는 설계(p01)와 AI 오류 사례 읽기(p10)는 공통적으로 **AI를 사고의 대체물이 아니라 분석 대상**으로 둔다.\n\n- 질문형 튜터(p03)도 같은 맥락에서 볼 수 있다.\n- 내 수업 적용 아이디어: 매주 AI 답변 1개를 반박하는 짧은 과제", vec![p01.clone(), p03.clone()], vec!["수업 설계"]),
        ("[데모] AI 의존과 메타인지", "p02의 결과: 의존도가 높을수록 비판적 사고 기능이 낮고, 메타인지가 부분 매개.\n\n> 질문: 메타인지를 높이는 수업 개입은 무엇이 있을까?\n\n관련: [[[데모] 핵심 개념: 비판적 사고의 성향과 기능]]", vec![p02.clone()], vec!["메타인지"]),
        ("[데모] AI 리터러시와 정보 신뢰성", "p06: 4주 교육 후 환각 식별률 41% → 73% (가상 수치).\n\n- 측면 읽기 전략을 교양 수업에 넣을 수 있는지 검토", vec![p06.clone()], vec!["AI 리터러시"]),
        ("[데모] 연구 공백 메모", "p12(가상 체계적 문헌고찰)에 따르면 한 학기 이상 추적한 연구가 드물다.\n\n## 연구 공백 후보 (데모 자료 범위)\n\n1. 장기 추적 연구\n2. 수행 기반 비판적 사고 평가\n3. 전공별 비교\n\n※ 선택한 가상 자료 범위에서 도출한 후보입니다.", vec![p12.clone()], vec!["연구 공백"]),
    ];
    for (title, body, papers, tags) in notes {
        crate::notes::create(conn, &NoteInput { title: title.into(), body: body.into(), project_id: Some(project.id.clone()), tags: tags.iter().map(|s| s.to_string()).collect(), paper_ids: papers.into_iter().filter(|s| !s.is_empty()).collect(), from_ai_draft: false })?;
    }
    Ok(())
}
