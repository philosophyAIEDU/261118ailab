//! 연구 결과 내보내기: Word(DOCX), Excel(XLSX), CSV, Markdown, TXT, BibTeX, Obsidian 폴더.
//!
//! 모든 내보내기에는 생성 시각, 출처(논문·쪽), 검증 상태를 함께 기록한다.
//! 파일은 임시 파일에 먼저 쓴 뒤 이름을 바꿔 중간에 실패해도 깨진 파일이 남지 않게 한다.
use crate::compare::Comparison;
use crate::error::{AppError, AppResult};
use crate::library::Paper;
use crate::notes::Note;
use crate::review::Review;
use rusqlite::Connection;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Docx,
    Xlsx,
    Csv,
    Md,
    Txt,
    Bib,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Docx => "docx",
            Format::Xlsx => "xlsx",
            Format::Csv => "csv",
            Format::Md => "md",
            Format::Txt => "txt",
            Format::Bib => "bib",
        }
    }
}

/// 형식에 독립적인 문서 모델
#[derive(Debug, Clone)]
pub enum Block {
    Heading(usize, String),
    Para(String),
    Bullets(Vec<String>),
    Table { headers: Vec<String>, rows: Vec<Vec<String>> },
    Note(String),
}

pub struct Doc {
    pub title: String,
    pub blocks: Vec<Block>,
}

pub const BRAND: &str = "교수님만의 AI 연구실 (Philosophy Research Hub)";

fn provenance(ai: bool) -> Block {
    let mut s = format!("{BRAND}에서 생성 · {}", chrono::Local::now().format("%Y-%m-%d %H:%M"));
    if ai {
        s.push_str(" · AI가 생성한 내용은 검토용 초안이며 원문 대조가 필요합니다.");
    }
    Block::Note(s)
}

fn write_atomic(path: &Path, f: impl FnOnce(&Path) -> AppResult<()>) -> AppResult<()> {
    let tmp = path.with_extension(format!("{}.tmp", path.extension().and_then(|e| e.to_str()).unwrap_or("x")));
    f(&tmp)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn md_escape_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', "<br>")
}

pub fn to_markdown(d: &Doc) -> String {
    let mut s = format!("# {}\n\n", d.title);
    for b in &d.blocks {
        match b {
            Block::Heading(l, t) => s.push_str(&format!("{} {}\n\n", "#".repeat((*l + 1).min(6)), t)),
            Block::Para(t) => s.push_str(&format!("{t}\n\n")),
            Block::Note(t) => s.push_str(&format!("> {t}\n\n")),
            Block::Bullets(v) => {
                for x in v {
                    s.push_str(&format!("- {x}\n"));
                }
                s.push('\n');
            }
            Block::Table { headers, rows } => {
                s.push_str(&format!("| {} |\n", headers.iter().map(|h| md_escape_cell(h)).collect::<Vec<_>>().join(" | ")));
                s.push_str(&format!("|{}\n", "---|".repeat(headers.len())));
                for r in rows {
                    s.push_str(&format!("| {} |\n", r.iter().map(|c| md_escape_cell(c)).collect::<Vec<_>>().join(" | ")));
                }
                s.push('\n');
            }
        }
    }
    s
}

pub fn to_text(d: &Doc) -> String {
    let mut s = format!("{}\n{}\n\n", d.title, "=".repeat(d.title.chars().count().min(60)));
    for b in &d.blocks {
        match b {
            Block::Heading(_, t) => s.push_str(&format!("■ {t}\n\n")),
            Block::Para(t) | Block::Note(t) => s.push_str(&format!("{t}\n\n")),
            Block::Bullets(v) => {
                for x in v {
                    s.push_str(&format!("  • {x}\n"));
                }
                s.push('\n');
            }
            Block::Table { headers, rows } => {
                for r in rows {
                    for (h, c) in headers.iter().zip(r) {
                        s.push_str(&format!("  {h}: {c}\n"));
                    }
                    s.push('\n');
                }
            }
        }
    }
    s
}

fn docx_text_paragraph(t: &str, size: usize, bold: bool, color: Option<&str>) -> docx_rs::Paragraph {
    use docx_rs::*;
    let mut p = Paragraph::new();
    for (i, line) in t.split('\n').enumerate() {
        let mut r = Run::new().add_text(line).size(size);
        if bold {
            r = r.bold();
        }
        if let Some(c) = color {
            r = r.color(c);
        }
        if i > 0 {
            r = Run::new().add_break(BreakType::TextWrapping).add_text(line).size(size);
        }
        p = p.add_run(r);
    }
    p
}

pub fn write_docx(d: &Doc, path: &Path) -> AppResult<()> {
    use docx_rs::*;
    let mut doc = Docx::new().default_fonts(RunFonts::new().ascii("Calibri").east_asia("Malgun Gothic").hi_ansi("Calibri"));
    doc = doc.add_paragraph(docx_text_paragraph(&d.title, 36, true, Some("17263D")));
    for b in &d.blocks {
        doc = match b {
            Block::Heading(l, t) => doc.add_paragraph(docx_text_paragraph(t, if *l <= 1 { 28 } else { 24 }, true, Some("17263D"))),
            Block::Para(t) => doc.add_paragraph(docx_text_paragraph(&strip_md(t), 21, false, None)),
            Block::Note(t) => doc.add_paragraph(docx_text_paragraph(t, 18, false, Some("6B6B6B"))),
            Block::Bullets(v) => {
                let mut dd = doc;
                for x in v {
                    dd = dd.add_paragraph(docx_text_paragraph(&format!("• {}", strip_md(x)), 21, false, None));
                }
                dd
            }
            Block::Table { headers, rows } => {
                let head = TableRow::new(headers.iter().map(|h| TableCell::new().shading(Shading::new().fill("EDE7DA")).add_paragraph(docx_text_paragraph(h, 19, true, None))).collect());
                let mut trs = vec![head];
                for r in rows {
                    trs.push(TableRow::new(r.iter().map(|c| TableCell::new().add_paragraph(docx_text_paragraph(c, 18, false, None))).collect()));
                }
                doc.add_table(Table::new(trs)).add_paragraph(Paragraph::new())
            }
        };
    }
    write_atomic(path, |tmp| {
        let f = std::fs::File::create(tmp)?;
        doc.build().pack(f).map_err(|_| AppError::File("Word 파일을 만들지 못했습니다".into()))?;
        Ok(())
    })
}

/// Markdown 강조 기호 정도만 제거 (Word 본문용)
fn strip_md(s: &str) -> String {
    s.lines()
        .map(|l| {
            let l = l.trim_start_matches('#').trim_start();
            l.replace("**", "").replace("__", "")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn write_xlsx(sheets: &[(&str, Vec<String>, Vec<Vec<String>>)], note: &str, path: &Path) -> AppResult<()> {
    use rust_xlsxwriter::*;
    let mut wb = Workbook::new();
    let head = Format::new().set_bold().set_background_color(Color::RGB(0xEDE7DA)).set_text_wrap().set_border(FormatBorder::Thin);
    let wrap = Format::new().set_text_wrap().set_align(FormatAlign::Top).set_border(FormatBorder::Thin);
    let err = |_| AppError::File("Excel 파일을 만들지 못했습니다".into());
    for (name, headers, rows) in sheets {
        let ws = wb.add_worksheet();
        let safe: String = name.chars().filter(|c| !"[]:*?/\\".contains(*c)).take(31).collect();
        ws.set_name(if safe.is_empty() { "Sheet" } else { &safe }).map_err(err)?;
        for (c, h) in headers.iter().enumerate() {
            ws.write_string_with_format(0, c as u16, h, &head).map_err(err)?;
            ws.set_column_width(c as u16, if c == 0 { 28 } else { 40 }).map_err(err)?;
        }
        for (r, row) in rows.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                ws.write_string_with_format(r as u32 + 1, c as u16, v, &wrap).map_err(err)?;
            }
        }
        ws.write_string(rows.len() as u32 + 2, 0, note).map_err(err)?;
        ws.set_freeze_panes(1, 1).map_err(err)?;
    }
    write_atomic(path, |tmp| wb.save(tmp).map_err(|_| AppError::File("Excel 파일을 저장하지 못했습니다".into())))
}

pub fn write_csv(headers: &[String], rows: &[Vec<String>], path: &Path) -> AppResult<()> {
    write_atomic(path, |tmp| {
        // Excel에서 한글이 깨지지 않도록 UTF-8 BOM
        let mut f = std::fs::File::create(tmp)?;
        use std::io::Write;
        f.write_all(&[0xEF, 0xBB, 0xBF])?;
        let mut w = csv::Writer::from_writer(f);
        w.write_record(headers).map_err(|_| AppError::File("CSV 파일을 만들지 못했습니다".into()))?;
        for r in rows {
            w.write_record(r).map_err(|_| AppError::File("CSV 파일을 만들지 못했습니다".into()))?;
        }
        w.flush()?;
        Ok(())
    })
}

pub fn write_string(s: &str, path: &Path) -> AppResult<()> {
    write_atomic(path, |tmp| Ok(std::fs::write(tmp, s)?))
}

fn write_doc(d: &Doc, fmt: Format, path: &Path) -> AppResult<()> {
    match fmt {
        Format::Docx => write_docx(d, path),
        Format::Md => write_string(&to_markdown(d), path),
        Format::Txt => write_string(&to_text(d), path),
        _ => Err(AppError::Unsupported("이 항목은 해당 형식으로 내보낼 수 없습니다".into())),
    }
}

// ───────────── 논문 목록 ─────────────

fn source_label(s: Option<&String>) -> &'static str {
    match s.map(String::as_str) {
        Some("user") => "직접 입력",
        Some("pdf_info") => "PDF 속성",
        Some("text") => "본문 표기",
        Some("text_guess") => "자동 추정(확인 필요)",
        Some("file_name") => "파일 이름",
        Some("bibtex") => "BibTeX",
        Some("ris") => "RIS",
        _ => "",
    }
}

fn status_label(s: &str) -> &'static str {
    match s {
        "ok" => "본문 추출 완료",
        "partial" => "일부 쪽 글자 없음",
        "no_text" => "글자 없음(OCR 필요)",
        "failed" => "본문 추출 실패",
        "metadata_only" => "서지정보만",
        _ => "처리 대기",
    }
}

pub fn papers_table(papers: &[Paper]) -> (Vec<String>, Vec<Vec<String>>) {
    let headers = ["제목", "저자", "연도", "학술지", "DOI", "키워드", "연구 분야", "태그", "쪽수", "본문 상태", "제목 출처", "초록", "개인 메모"].iter().map(|s| s.to_string()).collect();
    let rows = papers
        .iter()
        .map(|p| {
            vec![
                p.title.clone(),
                p.authors.join("; "),
                p.year.map(|y| y.to_string()).unwrap_or_default(),
                p.journal.clone(),
                p.doi.clone(),
                p.keywords.join(", "),
                p.field.clone(),
                p.tags.join(", "),
                p.page_count.map(|n| n.to_string()).unwrap_or_default(),
                status_label(&p.text_status).into(),
                source_label(p.meta_source.get("title")).into(),
                p.abstract_text.clone(),
                p.memo.clone(),
            ]
        })
        .collect();
    (headers, rows)
}

pub fn paper_bibentry(p: &Paper) -> crate::bib::BibEntry {
    crate::bib::BibEntry {
        entry_type: if p.journal.is_empty() { "misc".into() } else { "article".into() },
        key: if p.bib_key.is_empty() { crate::bib::make_key(&p.authors, p.year, &p.title) } else { p.bib_key.clone() },
        title: p.title.clone(),
        authors: p.authors.clone(),
        year: p.year,
        journal: p.journal.clone(),
        doi: p.doi.clone(),
        abstract_text: p.abstract_text.clone(),
        keywords: p.keywords.clone(),
        ..Default::default()
    }
}

pub fn export_papers(papers: &[Paper], fmt: Format, path: &Path) -> AppResult<()> {
    let (h, r) = papers_table(papers);
    match fmt {
        Format::Xlsx => write_xlsx(&[("논문 목록", h, r)], &format!("{BRAND}에서 내보냄 · {}", chrono::Local::now().format("%Y-%m-%d")), path),
        Format::Csv => write_csv(&h, &r, path),
        Format::Bib => {
            let mut keys = std::collections::HashSet::new();
            let mut s = format!("% {BRAND}에서 내보냄\n\n");
            for p in papers {
                let mut e = paper_bibentry(p);
                let base = e.key.clone();
                let mut n = 1;
                while !keys.insert(e.key.clone()) {
                    n += 1;
                    e.key = format!("{base}{}", (b'a' + (n as u8 - 2) % 26) as char);
                }
                s.push_str(&crate::bib::to_bibtex(&e));
                s.push('\n');
            }
            write_string(&s, path)
        }
        _ => {
            let short: Vec<Vec<String>> = r.iter().map(|x| vec![x[0].clone(), x[1].clone(), x[2].clone(), x[3].clone(), x[5].clone(), x[9].clone()]).collect();
            let d = Doc {
                title: "논문 목록".into(),
                blocks: vec![provenance(false), Block::Para(format!("총 {}편", papers.len())), Block::Table { headers: vec!["제목".into(), "저자".into(), "연도".into(), "학술지".into(), "키워드".into(), "본문 상태".into()], rows: short }],
            };
            write_doc(&d, fmt, path)
        }
    }
}

// ───────────── 비교표 ─────────────

fn cell_text(c: Option<&crate::compare::Cell>) -> String {
    match c {
        Some(c) if c.status == "found" => {
            let pages: Vec<String> = c.evidence.iter().map(|e| e.page.map(|p| format!("p.{p}")).unwrap_or_else(|| "쪽 정보 없음".into())).collect();
            format!("{} [근거: {}]", c.value, pages.join(", "))
        }
        Some(c) if c.status == "weak" => {
            let pages: Vec<String> = c.evidence.iter().map(|e| e.page.map(|p| format!("p.{p}")).unwrap_or_default()).collect();
            format!("{} [원문 확인 필요: {}]", c.value, pages.join(", "))
        }
        _ => "원문에서 확인되지 않음".into(),
    }
}

pub fn comparison_table(c: &Comparison) -> (Vec<String>, Vec<Vec<String>>) {
    let mut headers = vec!["비교 항목".to_string()];
    headers.extend(c.columns.iter().map(|col| format!("{}{}", col.title, col.year.map(|y| format!(" ({y})")).unwrap_or_default())));
    let rows = c.aspects.iter().map(|(k, label)| {
        let mut r = vec![label.clone()];
        r.extend(c.columns.iter().map(|col| cell_text(col.cells.get(k))));
        r
    }).collect();
    (headers, rows)
}

pub fn export_comparison(c: &Comparison, fmt: Format, path: &Path) -> AppResult<()> {
    let (h, r) = comparison_table(c);
    // 근거 원문 시트/절
    let ev_headers: Vec<String> = ["논문", "항목", "검증 상태", "쪽", "원문 발췌"].iter().map(|s| s.to_string()).collect();
    let mut ev_rows = vec![];
    for col in &c.columns {
        for (k, label) in &c.aspects {
            if let Some(cell) = col.cells.get(k) {
                for e in &cell.evidence {
                    ev_rows.push(vec![col.title.clone(), label.clone(), if cell.status == "found" { "근거 확인".into() } else { "원문 확인 필요".into() }, e.page.map(|p| p.to_string()).unwrap_or_default(), e.quote.clone()]);
                }
            }
        }
    }
    let note = format!("{BRAND} · 모델: {} · {} · AI가 정리한 비교표입니다. 각 칸의 근거 쪽을 원문과 대조해 주세요.", c.model, c.created_at);
    match fmt {
        Format::Xlsx => write_xlsx(&[("비교표", h, r), ("근거 원문", ev_headers, ev_rows)], &note, path),
        Format::Csv => write_csv(&h, &r, path),
        _ => {
            let d = Doc { title: c.title.clone(), blocks: vec![provenance(true), Block::Note(format!("모델: {}", c.model)), Block::Table { headers: h, rows: r }, Block::Heading(1, "근거 원문".into()), Block::Table { headers: ev_headers, rows: ev_rows }] };
            write_doc(&d, fmt, path)
        }
    }
}

// ───────────── 노트 ─────────────

pub fn note_frontmatter(n: &Note, papers: &[Paper], project: Option<&str>) -> String {
    let q = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let mut s = String::from("---\n");
    s.push_str(&format!("title: {}\n", q(&n.title)));
    s.push_str(&format!("created: {}\n", n.created_at));
    s.push_str(&format!("updated: {}\n", n.updated_at));
    if !n.tags.is_empty() {
        s.push_str("tags:\n");
        for t in &n.tags {
            s.push_str(&format!("  - {}\n", q(&t.replace(' ', "-"))));
        }
    }
    if let Some(p) = project {
        s.push_str(&format!("project: {}\n", q(p)));
    }
    if !papers.is_empty() {
        s.push_str("papers:\n");
        for p in papers {
            s.push_str(&format!("  - {}\n", q(&format!("[[{}]]", safe_file_stem(&p.title)))));
        }
    }
    if n.origin == "ai_reviewed" {
        s.push_str("origin: \"AI 초안 검토 후 저장\"\n");
    }
    s.push_str("source: \"교수님만의 AI 연구실\"\n---\n\n");
    s
}

pub fn safe_file_stem(s: &str) -> String {
    let t: String = s.chars().map(|c| if "\\/:*?\"<>|#^[]".contains(c) || c.is_control() { ' ' } else { c }).collect();
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    let t: String = t.chars().take(80).collect();
    let t = t.trim_matches('.').trim().to_string();
    if t.is_empty() { "untitled".into() } else { t }
}

/// 기존 파일을 덮어쓰지 않도록 겹치지 않는 경로를 고른다
pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut p = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while p.exists() {
        p = dir.join(format!("{stem} ({n}).{ext}"));
        n += 1;
    }
    p
}

pub fn export_notes(notes: &[Note], conn: &Connection, fmt: Format, path: &Path) -> AppResult<()> {
    let mut blocks = vec![provenance(false)];
    for n in notes {
        let papers: Vec<Paper> = n.paper_ids.iter().filter_map(|id| crate::library::get_paper(conn, id).ok()).collect();
        blocks.push(Block::Heading(1, n.title.clone()));
        let mut meta = vec![format!("작성 {} · 수정 {}", &n.created_at[..10.min(n.created_at.len())], &n.updated_at[..10.min(n.updated_at.len())])];
        if !n.tags.is_empty() {
            meta.push(format!("태그: {}", n.tags.join(", ")));
        }
        if n.origin == "ai_reviewed" {
            meta.push("AI 초안을 검토 후 저장한 노트".into());
        }
        blocks.push(Block::Note(meta.join(" · ")));
        blocks.push(Block::Para(n.body.clone()));
        if !papers.is_empty() {
            blocks.push(Block::Bullets(papers.iter().map(|p| format!("관련 논문: {}", crate::review::format_reference(p))).collect()));
        }
    }
    let d = Doc { title: if notes.len() == 1 { notes[0].title.clone() } else { format!("연구 노트 ({}개)", notes.len()) }, blocks };
    if fmt == Format::Md && notes.len() == 1 {
        let n = &notes[0];
        let papers: Vec<Paper> = n.paper_ids.iter().filter_map(|id| crate::library::get_paper(conn, id).ok()).collect();
        return write_string(&format!("{}# {}\n\n{}\n", note_frontmatter(n, &papers, None), n.title, n.body), path);
    }
    write_doc(&d, fmt, path)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ObsidianReport {
    pub folder: String,
    pub notes_written: usize,
    pub papers_written: usize,
    pub skipped_existing: usize,
}

/// Obsidian 보관함으로 내보내기. 선택한 폴더 안에 `교수님만의 AI 연구실/` 폴더를 만들고
/// 프로젝트별 하위 폴더에 노트를, `논문/` 폴더에 논문 요약 노트를 쓴다. 기존 파일은 덮어쓰지 않는다.
pub fn export_obsidian(conn: &Connection, vault: &Path) -> AppResult<ObsidianReport> {
    if !vault.is_dir() {
        return Err(AppError::Invalid("내보낼 폴더를 찾을 수 없습니다".into()));
    }
    let root = vault.join("교수님만의 AI 연구실");
    let papers_dir = root.join("논문");
    std::fs::create_dir_all(&papers_dir)?;
    let projects = crate::projects::list(conn)?;
    let notes = crate::notes::list(conn, &Default::default())?;
    let mut rep = ObsidianReport { folder: root.to_string_lossy().into_owned(), notes_written: 0, papers_written: 0, skipped_existing: 0 };
    let all_papers = crate::library::list_papers(conn, &Default::default())?;
    for p in &all_papers {
        let stem = safe_file_stem(&p.title);
        let path = papers_dir.join(format!("{stem}.md"));
        if path.exists() {
            rep.skipped_existing += 1;
            continue;
        }
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\\\""));
        let mut s = format!("---\ntitle: {}\n", q(&p.title));
        if !p.authors.is_empty() {
            s.push_str(&format!("authors: [{}]\n", p.authors.iter().map(|a| q(a)).collect::<Vec<_>>().join(", ")));
        }
        if let Some(y) = p.year {
            s.push_str(&format!("year: {y}\n"));
        }
        if !p.doi.is_empty() {
            s.push_str(&format!("doi: {}\n", q(&p.doi)));
        }
        let tags: Vec<String> = p.tags.iter().chain(p.keywords.iter()).map(|t| q(&t.replace(' ', "-"))).collect();
        if !tags.is_empty() {
            s.push_str(&format!("tags: [{}]\n", tags.join(", ")));
        }
        s.push_str("type: paper\nsource: \"교수님만의 AI 연구실\"\n---\n\n");
        s.push_str(&format!("# {}\n\n{}\n\n", p.title, crate::review::format_reference(p)));
        if !p.abstract_text.is_empty() {
            s.push_str(&format!("## 초록\n\n{}\n\n", p.abstract_text));
        }
        if !p.memo.is_empty() {
            s.push_str(&format!("## 개인 메모\n\n{}\n", p.memo));
        }
        std::fs::write(&path, s)?;
        rep.papers_written += 1;
    }
    for n in &notes {
        let proj = n.project_id.as_ref().and_then(|pid| projects.iter().find(|p| &p.id == pid));
        let dir = match proj {
            Some(p) => root.join(safe_file_stem(&p.name)),
            None => root.join("연구 노트"),
        };
        std::fs::create_dir_all(&dir)?;
        let stem = safe_file_stem(&n.title);
        let path = dir.join(format!("{stem}.md"));
        if path.exists() {
            rep.skipped_existing += 1;
            continue;
        }
        let papers: Vec<Paper> = n.paper_ids.iter().filter_map(|id| all_papers.iter().find(|p| &p.id == id).cloned()).collect();
        let mut body = format!("{}# {}\n\n{}\n", note_frontmatter(n, &papers, proj.map(|p| p.name.as_str())), n.title, n.body);
        if !papers.is_empty() {
            body.push_str("\n## 관련 논문\n\n");
            for p in &papers {
                body.push_str(&format!("- [[{}]]\n", safe_file_stem(&p.title)));
            }
        }
        std::fs::write(&path, body)?;
        rep.notes_written += 1;
    }
    Ok(rep)
}

// ───────────── 문헌고찰 / 검색 결과 / 프로젝트 보고서 ─────────────

pub fn export_review(r: &Review, fmt: Format, path: &Path) -> AppResult<()> {
    let mut blocks = vec![provenance(true), Block::Note(r.notice.clone()), Block::Heading(1, "연구 주제별 분류 (규칙 기반)".into())];
    blocks.push(Block::Bullets(r.groups.iter().map(|g| format!("{} — {}편", g.label, g.paper_ids.len())).collect()));
    blocks.push(Block::Heading(1, "개요 초안".into()));
    for part in r.draft.split("\n\n") {
        let t = part.trim();
        if let Some(h) = t.strip_prefix("## ") {
            blocks.push(Block::Heading(2, h.to_string()));
        } else if !t.is_empty() {
            blocks.push(Block::Para(t.to_string()));
        }
    }
    blocks.push(Block::Heading(1, "근거 출처".into()));
    blocks.push(Block::Table {
        headers: vec!["표시".into(), "논문".into(), "쪽".into(), "검증".into(), "원문 발췌".into()],
        rows: r.citations.iter().map(|c| vec![c.label.clone(), c.paper_title.clone(), c.page.map(|p| p.to_string()).unwrap_or_default(), if c.support == "verified" { "문장-원문 대응 확인".into() } else { "원문 확인 필요".into() }, c.quote.clone()]).collect(),
    });
    if r.removed_markers > 0 || !r.removed_references.is_empty() {
        blocks.push(Block::Note(format!("검증 과정에서 존재하지 않는 출처 표기 {}개와 확인되지 않은 인용 {}개를 제거했습니다.", r.removed_markers, r.removed_references.len())));
    }
    blocks.push(Block::Heading(1, "참고문헌 (등록된 서지정보 기준)".into()));
    blocks.push(Block::Bullets(r.references.clone()));
    write_doc(&Doc { title: r.title.clone(), blocks }, fmt, path)
}

pub fn export_ask(r: &crate::rag::AskResult, fmt: Format, path: &Path) -> AppResult<()> {
    let mut blocks = vec![provenance(r.status == "answered"), Block::Heading(1, "질문".into()), Block::Para(r.question.clone()), Block::Heading(1, "AI 답변".into()), Block::Para(r.answer.clone())];
    blocks.push(Block::Heading(1, "근거 (실제 검색된 원문)".into()));
    blocks.push(Block::Table {
        headers: vec!["표시".into(), "논문".into(), "쪽".into(), "검증".into(), "원문".into()],
        rows: r.citations.iter().map(|c| vec![c.label.clone(), c.source.paper_title.clone(), c.source.page.map(|p| p.to_string()).unwrap_or_default(), if c.support == "verified" { "문장-원문 대응 확인".into() } else { "원문 확인 필요".into() }, crate::text::preview(&c.source.text, 400)]).collect(),
    });
    if !r.other_sources.is_empty() {
        blocks.push(Block::Heading(1, "함께 검색된 자료".into()));
        blocks.push(Block::Bullets(r.other_sources.iter().map(|s| format!("{} (p.{}) — {}", s.paper_title, s.page.map(|p| p.to_string()).unwrap_or("-".into()), crate::text::preview(&s.text, 160))).collect()));
    }
    write_doc(&Doc { title: "AI 연구 검색 결과".into(), blocks }, fmt, path)
}

pub fn export_project(conn: &Connection, project_id: &str, fmt: Format, path: &Path) -> AppResult<()> {
    let p = crate::projects::get(conn, project_id)?;
    let papers: Vec<Paper> = p.paper_ids.iter().filter_map(|id| crate::library::get_paper(conn, id).ok()).collect();
    let notes = crate::notes::list(conn, &crate::notes::NoteFilter { project_id: Some(p.id.clone()), ..Default::default() })?;
    let status = match p.status.as_str() { "done" => "완료", "paused" => "보류", _ => "진행 중" };
    let mut blocks = vec![provenance(false), Block::Para(format!("상태: {status}")), Block::Para(p.description.clone()), Block::Heading(1, format!("연구 자료 ({}편)", papers.len()))];
    let (h, r) = papers_table(&papers);
    let keep = [0usize, 1, 2, 3, 5, 9];
    blocks.push(Block::Table { headers: keep.iter().map(|i| h[*i].clone()).collect(), rows: r.iter().map(|row| keep.iter().map(|i| row[*i].clone()).collect()).collect() });
    blocks.push(Block::Heading(1, format!("연구 노트 ({}개)", notes.len())));
    for n in &notes {
        blocks.push(Block::Heading(2, n.title.clone()));
        blocks.push(Block::Para(n.body.clone()));
    }
    let comps: Vec<Comparison> = crate::compare::list(conn)?.into_iter().filter(|c| c.columns.iter().any(|col| p.paper_ids.contains(&col.paper_id))).collect();
    if !comps.is_empty() {
        blocks.push(Block::Heading(1, "논문 비교".into()));
        for c in &comps {
            blocks.push(Block::Heading(2, c.title.clone()));
            let (h, r) = comparison_table(c);
            blocks.push(Block::Table { headers: h, rows: r });
        }
    }
    blocks.push(Block::Heading(1, "참고문헌".into()));
    let mut refs: Vec<String> = papers.iter().map(crate::review::format_reference).collect();
    refs.sort();
    blocks.push(Block::Bullets(refs));
    write_doc(&Doc { title: format!("연구 프로젝트 보고서 — {}", p.name), blocks }, fmt, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::{lib, write_pdf};

    #[test]
    fn exports_real_files() {
        let (d, paths, mut c) = lib();
        let a = write_pdf(d.path(), "a.pdf", &["Keywords: AI, writing\nGenerative AI essay study with long enough text"], Some("Generative AI in Essay Writing"));
        let id = crate::library::import_file(&mut c, &paths, &a).unwrap().paper_id.unwrap();
        let papers = crate::library::list_papers(&c, &Default::default()).unwrap();
        let out = d.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        for f in [Format::Docx, Format::Xlsx, Format::Csv, Format::Md, Format::Txt, Format::Bib] {
            let p = out.join(format!("papers.{}", f.ext()));
            export_papers(&papers, f, &p).unwrap();
            assert!(std::fs::metadata(&p).unwrap().len() > 50, "{:?}", f);
        }
        // docx는 zip이고 본문에 제목이 있어야 함
        let mut z = zip::ZipArchive::new(std::fs::File::open(out.join("papers.docx")).unwrap()).unwrap();
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut z.by_name("word/document.xml").unwrap(), &mut xml).unwrap();
        assert!(xml.contains("Generative AI in Essay Writing"));
        let bib = std::fs::read_to_string(out.join("papers.bib")).unwrap();
        assert_eq!(crate::bib::parse_bibtex(&bib)[0].title, "Generative AI in Essay Writing");
        let csv = std::fs::read(out.join("papers.csv")).unwrap();
        assert_eq!(&csv[..3], &[0xEF, 0xBB, 0xBF]);
        // Obsidian: 기존 파일 덮어쓰지 않음
        let n = crate::notes::create(&mut c, &crate::notes::NoteInput { title: "첫 노트".into(), body: "내용 [[다른 노트]]".into(), project_id: None, tags: vec!["AI 교육".into()], paper_ids: vec![id], from_ai_draft: false }).unwrap();
        let vault = d.path().join("vault");
        std::fs::create_dir_all(vault.join("교수님만의 AI 연구실/연구 노트")).unwrap();
        std::fs::write(vault.join("교수님만의 AI 연구실/연구 노트/첫 노트.md"), "기존 내용").unwrap();
        let r = export_obsidian(&c, &vault).unwrap();
        assert_eq!(r.skipped_existing, 1);
        assert_eq!(r.papers_written, 1);
        assert_eq!(std::fs::read_to_string(vault.join("교수님만의 AI 연구실/연구 노트/첫 노트.md")).unwrap(), "기존 내용");
        let fm = note_frontmatter(&n.note, &papers, Some("프로젝트"));
        assert!(fm.starts_with("---\ntitle: \"첫 노트\""));
        assert!(fm.contains("  - \"AI-교육\""));
        assert!(fm.contains("[[Generative AI in Essay Writing]]"));
        assert_eq!(unique_path(&vault.join("교수님만의 AI 연구실/연구 노트"), "첫 노트", "md").file_name().unwrap().to_string_lossy(), "첫 노트 (2).md");
    }
}
