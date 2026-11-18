//! 실제 한글 PDF(데모 자료, Chromium으로 생성)에서 쪽별 본문·서지정보·인용 관계를 추출하는지 확인한다.
use prh_core::*;
use std::path::PathBuf;

fn demo_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../demo/papers")
}

#[test]
fn korean_demo_pdfs_extract_with_pages() {
    let dir = demo_dir();
    let e = extract::extract(&dir.join("p01.pdf")).unwrap();
    assert!(e.page_count.unwrap() >= 2, "쪽 수: {:?}", e.page_count);
    assert_eq!(e.status(), "ok", "{:?}", e.warnings);
    let all: String = e.pages.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("\n");
    assert!(all.contains("비판적 사고"), "추출 본문 앞부분: {}", text::preview(&all, 300));
    assert!(all.contains("128명"));
    let m = metadata::extract_metadata(&e, "p01.pdf");
    assert!(m.title.contains("생성형 AI 활용 글쓰기 수업"), "제목: {}", m.title);
    assert!(m.abstract_text.starts_with("본 연구는"), "초록: {}", m.abstract_text);
    assert!(m.keywords.contains(&"비판적 사고".to_string()), "키워드: {:?}", m.keywords);
}

#[test]
fn demo_library_import_and_citations() {
    let d = tempfile::tempdir().unwrap();
    let paths = paths::LibraryPaths::new(d.path().join("lib"));
    paths.ensure(true).unwrap();
    let (mut c, _) = db::open(&paths).unwrap();
    let files = library::collect_files(&demo_dir());
    let pdfs: Vec<_> = files.iter().filter(|p| p.extension().map(|e| e == "pdf").unwrap_or(false)).collect();
    assert_eq!(pdfs.len(), 12);
    for f in &pdfs {
        let o = library::import_file(&mut c, &paths, f).unwrap();
        assert!(matches!(o.status, library::ImportStatus::Added), "{} {:?}", o.file_name, o.message);
    }
    let n: i64 = c.query_row("SELECT count(*) FROM citations", [], |r| r.get(0)).unwrap();
    assert!(n >= 6, "인용 관계 수: {n}");
    let r = search::search(&c, &search::SearchParams { query: "생성형 AI와 비판적 사고의 관계를 다룬 연구를 찾아줘", mode: search::Mode::Keyword, limit: 8, per_paper: 2, papers: None }, None, None).unwrap();
    assert!(r.hits.len() >= 5);
    assert!(r.hits.iter().all(|h| h.page.is_some()));
    demo::seed(&mut c).unwrap();
    let st = library::stats(&c).unwrap();
    assert_eq!(st.notes, 5);
    assert_eq!(st.projects_active, 1);
    let g = graph::build(&c).unwrap();
    assert!(g.edges.iter().any(|e| e.kind == "citation"));
    assert!(g.nodes.iter().any(|n| n.kind == "concept"));
    let all = library::list_papers(&c, &library::PaperFilter { query: "구조방정식".into(), ..Default::default() }).unwrap();
    assert!(all.len() >= 2, "본문에서 언급한 논문도 검색됨");
    let p = all.iter().find(|p| p.title.contains("구조방정식 분석")).unwrap();
    assert_eq!(p.authors, vec!["데모저자 최바다"]);
    assert_eq!(p.meta_source["authors"], "demo");
}
