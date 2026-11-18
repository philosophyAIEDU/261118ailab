//! 서지정보(메타데이터) 자동 추출.
//!
//! 원칙: **문서 안에 실제로 있는 정보만** 가져온다. 확인할 수 없는 값은 비워 둔다.
//! 각 필드에는 출처를 기록한다.
//! - `pdf_info`  : PDF 파일 속성(Info 사전)에 기록된 값
//! - `text`      : 본문에서 표지("Abstract", "Keywords", DOI 형식 등)로 찾은 값
//! - `text_guess`: 본문 배치로 추정한 값 (예: 첫 쪽의 제목 후보) → 화면에 "확인 필요"로 표시
//! - `file_name` : 파일 이름에서 가져온 값
//! - `user`/`bibtex`/`ris`: 사용자가 입력했거나 서지 파일에서 가져온 값
use crate::extract::Extracted;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Metadata {
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub journal: String,
    pub doi: String,
    pub abstract_text: String,
    pub keywords: Vec<String>,
    /// 필드 이름 → 출처
    pub source: BTreeMap<String, String>,
}

fn doi_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"(?i)\b(10\.\d{4,9}/[^\s"<>\]\[,;]+)"#).unwrap())
}

pub fn find_doi(text: &str) -> Option<String> {
    doi_re().captures(text).map(|c| c[1].trim_end_matches(['.', ')']).to_string())
}

fn plausible_title(t: &str) -> bool {
    let l = t.to_lowercase();
    let n = t.chars().count();
    (6..=300).contains(&n)
        && !l.starts_with("microsoft word")
        && !l.ends_with(".doc")
        && !l.ends_with(".docx")
        && !l.ends_with(".pdf")
        && !l.contains("untitled")
        && l != "title"
        && t.chars().filter(|c| c.is_alphabetic()).count() >= 4
}

fn is_header_noise(line: &str) -> bool {
    let l = line.to_lowercase();
    let noise = [
        "journal", "vol.", "volume", "issn", "doi", "http", "www.", "©", "copyright", "received", "accepted", "published",
        "pp.", "issue", "학회", "학술지", "논문집", "제 ", "권", "호", "pissn", "eissn", "e-mail", "email", "@", "page",
        "arxiv", "preprint", "available online", "contents lists",
    ];
    noise.iter().any(|w| l.contains(w)) || l.chars().all(|c| c.is_ascii_digit() || c.is_whitespace() || c == '-')
}

fn split_list(s: &str) -> Vec<String> {
    s.split([',', ';', '·', '，', '、', '|'])
        .map(|x| x.trim().trim_end_matches('.').trim().to_string())
        .filter(|x| !x.is_empty() && x.chars().count() <= 80)
        .collect()
}

fn split_authors(s: &str) -> Vec<String> {
    let s = s.replace(" and ", ";").replace(" & ", ";");
    let parts: Vec<String> = if s.contains(';') { s.split(';').map(|x| x.trim().to_string()).collect() } else { split_list(&s) };
    parts.into_iter().filter(|a| !a.is_empty() && a.chars().count() <= 100).collect()
}

/// 본문에서 표지 단어 뒤의 내용을 찾는다
fn after_label<'a>(text: &'a str, labels: &[&str]) -> Option<&'a str> {
    let lower = text.to_lowercase();
    for lab in labels {
        let ll = lab.to_lowercase();
        let mut start = 0;
        while let Some(pos) = lower[start..].find(&ll) {
            let at = start + pos;
            // 줄 시작(또는 공백 뒤)에 있는 표지만 인정
            let prev_ok = at == 0 || lower[..at].ends_with(['\n', ' ', '\t', '[', '(']);
            let end = at + ll.len();
            if prev_ok && text.is_char_boundary(end) {
                let rest = &text[end..];
                let rest = rest.trim_start_matches([':', '：', ' ', '.', '-', '—', '\t']);
                return Some(rest);
            }
            start = end;
            if start >= lower.len() {
                break;
            }
        }
    }
    None
}

pub fn extract_metadata(e: &Extracted, file_name: &str) -> Metadata {
    let mut m = Metadata::default();
    let first_pages: String = e.pages.iter().take(2).map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("\n\n");

    // 제목
    if let Some(t) = e.info.title.as_ref().filter(|t| plausible_title(t)) {
        m.title = t.clone();
        m.source.insert("title".into(), "pdf_info".into());
    } else if let Some(first) = e.pages.first() {
        let lines: Vec<&str> = first.1.lines().map(str::trim).filter(|l| !l.is_empty()).take(15).collect();
        if let Some(line) = lines.iter().find(|l| {
            let n = l.chars().count();
            (10..=200).contains(&n) && !is_header_noise(l) && !l.to_lowercase().starts_with("abstract")
        }) {
            m.title = line.to_string();
            m.source.insert("title".into(), "text_guess".into());
        }
    }
    if m.title.is_empty() {
        m.title = std::path::Path::new(file_name).file_stem().map(|s| s.to_string_lossy().replace(['_'], " ")).unwrap_or_default();
        m.source.insert("title".into(), "file_name".into());
    }

    // 저자 — PDF 속성에 있을 때만 (본문에서 저자를 추측하지 않는다)
    if let Some(a) = &e.info.author {
        let list = split_authors(a);
        if !list.is_empty() {
            m.authors = list;
            m.source.insert("authors".into(), "pdf_info".into());
        }
    }

    // DOI
    if let Some(d) = find_doi(&first_pages) {
        m.doi = d;
        m.source.insert("doi".into(), "text".into());
    }

    // 초록
    if let Some(rest) = after_label(&first_pages, &["Abstract", "ABSTRACT", "국문초록", "국문 초록", "초록", "요약", "要約"]) {
        let stops = ["keywords", "key words", "index terms", "주제어", "핵심어", "키워드", "1. introduction", "introduction\n", "i. introduction", "Ⅰ. 서론", "i. 서론", "1. 서론", "서론\n"];
        let lower = rest.to_lowercase();
        let end = stops.iter().filter_map(|s| lower.find(&s.to_lowercase())).min().unwrap_or(rest.len()).min(rest.len());
        let mut abs: String = rest[..end].chars().take(2500).collect();
        abs = abs.split_whitespace().collect::<Vec<_>>().join(" ");
        if abs.chars().count() >= 60 {
            m.abstract_text = abs;
            m.source.insert("abstract".into(), "text".into());
        }
    }

    // 키워드
    if let Some(rest) = after_label(&first_pages, &["Keywords", "KEYWORDS", "Key words", "Index Terms", "주제어", "핵심어", "키워드"]) {
        let line = rest.lines().next().unwrap_or("");
        let list = split_list(line);
        if !list.is_empty() && list.len() <= 15 {
            m.keywords = list;
            m.source.insert("keywords".into(), "text".into());
        }
    }
    if m.keywords.is_empty() {
        if let Some(k) = &e.info.keywords {
            let list = split_list(k);
            if !list.is_empty() {
                m.keywords = list;
                m.source.insert("keywords".into(), "pdf_info".into());
            }
        }
    }

    // 출판 연도 — 첫 쪽의 저작권/출판 표기 근처의 연도만 추정값으로 사용
    if let Some(first) = e.pages.first() {
        static Y: OnceLock<Regex> = OnceLock::new();
        let y = Y.get_or_init(|| Regex::new(r"(?i)(?:©|\(c\)|copyright|published|발행|게재|received|accepted)[^\n]{0,40}?\b((?:19|20)\d{2})\b").unwrap());
        if let Some(c) = y.captures(&first.1) {
            if let Ok(v) = c[1].parse::<i32>() {
                m.year = Some(v);
                m.source.insert("year".into(), "text_guess".into());
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::{Extracted, PdfInfo};

    fn ex(pages: &[&str], info: PdfInfo) -> Extracted {
        Extracted {
            kind: "pdf".into(),
            pages: pages.iter().enumerate().map(|(i, t)| (Some(i as u32 + 1), t.to_string())).collect(),
            page_count: Some(pages.len() as u32),
            info,
            empty_pages: vec![],
            warnings: vec![],
        }
    }

    #[test]
    fn extracts_labeled_fields() {
        let p1 = "Journal of Education Vol. 3\nGenerative AI and Critical Thinking among Undergraduates\nAbstract\nThis study examines how generative AI tools influence the critical thinking of university students in writing courses over one semester.\nKeywords: generative AI, critical thinking; higher education\n© 2024 Example Press. https://doi.org/10.1234/abcd.5678.";
        let m = extract_metadata(&ex(&[p1], PdfInfo::default()), "x.pdf");
        assert_eq!(m.title, "Generative AI and Critical Thinking among Undergraduates");
        assert_eq!(m.source["title"], "text_guess");
        assert_eq!(m.doi, "10.1234/abcd.5678");
        assert!(m.abstract_text.starts_with("This study examines"));
        assert!(!m.abstract_text.to_lowercase().contains("keywords"));
        assert_eq!(m.keywords, vec!["generative AI", "critical thinking", "higher education"]);
        assert_eq!(m.year, Some(2024));
        assert!(m.authors.is_empty(), "본문에서 저자를 추측하지 않는다");
    }

    #[test]
    fn prefers_pdf_info_and_rejects_junk_titles() {
        let info = PdfInfo { title: Some("Microsoft Word - draft3.docx".into()), author: Some("Kim, A.; Lee, B.".into()), ..Default::default() };
        let m = extract_metadata(&ex(&["short"], info), "my_paper.pdf");
        assert_eq!(m.title, "my paper");
        assert_eq!(m.source["title"], "file_name");
        assert_eq!(m.authors, vec!["Kim, A.", "Lee, B."]);
    }

    #[test]
    fn korean_labels() {
        let p1 = "생성형 인공지능 활용이 대학생의 비판적 사고에 미치는 영향\n국문초록\n본 연구는 생성형 인공지능을 활용한 글쓰기 수업이 대학생의 비판적 사고 성향에 미치는 영향을 분석하였다. 연구 결과 유의한 차이가 나타났다.\n주제어: 생성형 AI, 비판적 사고, 대학 교육";
        let m = extract_metadata(&ex(&[p1], PdfInfo::default()), "a.pdf");
        assert_eq!(m.title, "생성형 인공지능 활용이 대학생의 비판적 사고에 미치는 영향");
        assert!(m.abstract_text.starts_with("본 연구는"));
        assert_eq!(m.keywords, vec!["생성형 AI", "비판적 사고", "대학 교육"]);
    }
}
