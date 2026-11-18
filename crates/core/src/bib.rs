//! BibTeX / RIS 서지 파일 읽기·쓰기 (Zotero 내보내기 파일 호환).
//!
//! Zotero 내부 데이터베이스는 읽거나 수정하지 않는다. 사용자가 Zotero에서 내보낸 파일만 사용한다.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BibEntry {
    pub entry_type: String,
    pub key: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub journal: String,
    pub doi: String,
    pub abstract_text: String,
    pub keywords: Vec<String>,
    /// 서지 파일에 기록된 첨부 파일 경로 (Zotero `file` 필드 / RIS `L1`)
    pub files: Vec<String>,
    pub extra: BTreeMap<String, String>,
}

fn clean_latex(s: &str) -> String {
    let mut out = s.replace(['{', '}'], "").replace("\\&", "&").replace("\\%", "%").replace("\\_", "_").replace("~", " ").replace("--", "–");
    for (a, b) in [("\\\"a", "ä"), ("\\\"o", "ö"), ("\\\"u", "ü"), ("\\'e", "é"), ("\\`e", "è"), ("\\'a", "á"), ("\\c c", "ç")] {
        out = out.replace(a, b);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_year(s: &str) -> Option<i32> {
    let digits: String = s.chars().skip_while(|c| !c.is_ascii_digit()).take(4).collect();
    digits.parse().ok().filter(|y: &i32| (1000..=2100).contains(y))
}

fn split_kw(s: &str) -> Vec<String> {
    s.split([',', ';']).map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
}

/// Zotero의 file 필드: "설명:경로:MIME;..." 또는 단순 경로
fn parse_file_field(s: &str) -> Vec<String> {
    s.split(';')
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            let segs: Vec<&str> = part.split(':').collect();
            // Windows 드라이브 문자(C:\...)를 고려
            if segs.len() >= 3 {
                let mid = segs[1..segs.len() - 1].join(":");
                Some(mid.replace("\\:", ":"))
            } else {
                Some(part.to_string())
            }
        })
        .filter(|p| p.to_lowercase().ends_with(".pdf"))
        .collect()
}

pub fn parse_bibtex(src: &str) -> Vec<BibEntry> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        if chars[i] != '@' {
            i += 1;
            continue;
        }
        i += 1;
        let ts = i;
        while i < chars.len() && chars[i].is_alphanumeric() {
            i += 1;
        }
        let etype: String = chars[ts..i].iter().collect::<String>().to_lowercase();
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() || (chars[i] != '{' && chars[i] != '(') {
            continue;
        }
        let close = if chars[i] == '{' { '}' } else { ')' };
        i += 1;
        if etype == "comment" || etype == "preamble" || etype == "string" {
            let mut depth = 1;
            while i < chars.len() && depth > 0 {
                if chars[i] == '{' { depth += 1 } else if chars[i] == '}' { depth -= 1 }
                i += 1;
            }
            continue;
        }
        let ks = i;
        while i < chars.len() && chars[i] != ',' && chars[i] != close {
            i += 1;
        }
        let key: String = chars[ks..i].iter().collect::<String>().trim().to_string();
        let mut fields: BTreeMap<String, String> = BTreeMap::new();
        loop {
            while i < chars.len() && (chars[i].is_whitespace() || chars[i] == ',') {
                i += 1;
            }
            if i >= chars.len() || chars[i] == close {
                i += 1;
                break;
            }
            let ns = i;
            while i < chars.len() && chars[i] != '=' && chars[i] != close {
                i += 1;
            }
            if i >= chars.len() || chars[i] == close {
                i += 1;
                break;
            }
            let name: String = chars[ns..i].iter().collect::<String>().trim().to_lowercase();
            i += 1;
            let mut value = String::new();
            // 값: {..}, "..", 숫자/매크로, # 연결
            loop {
                while i < chars.len() && chars[i].is_whitespace() {
                    i += 1;
                }
                if i >= chars.len() {
                    break;
                }
                match chars[i] {
                    '{' => {
                        let mut depth = 1;
                        i += 1;
                        let vs = i;
                        while i < chars.len() && depth > 0 {
                            match chars[i] {
                                '{' => depth += 1,
                                '}' => depth -= 1,
                                _ => {}
                            }
                            i += 1;
                        }
                        value.push_str(&chars[vs..i.saturating_sub(1)].iter().collect::<String>());
                    }
                    '"' => {
                        i += 1;
                        let vs = i;
                        let mut depth = 0;
                        while i < chars.len() && !(chars[i] == '"' && depth == 0) {
                            match chars[i] {
                                '{' => depth += 1,
                                '}' => depth -= 1,
                                _ => {}
                            }
                            i += 1;
                        }
                        value.push_str(&chars[vs..i.min(chars.len())].iter().collect::<String>());
                        i += 1;
                    }
                    _ => {
                        let vs = i;
                        while i < chars.len() && !matches!(chars[i], ',' | '#') && chars[i] != close && !chars[i].is_whitespace() {
                            i += 1;
                        }
                        value.push_str(&chars[vs..i].iter().collect::<String>());
                    }
                }
                while i < chars.len() && chars[i].is_whitespace() {
                    i += 1;
                }
                if i < chars.len() && chars[i] == '#' {
                    i += 1;
                    continue;
                }
                break;
            }
            fields.insert(name, value);
        }
        let mut e = BibEntry { entry_type: etype, key, ..Default::default() };
        for (k, v) in fields {
            match k.as_str() {
                "title" => e.title = clean_latex(&v),
                "author" => e.authors = v.split(" and ").map(clean_latex).filter(|a| !a.is_empty()).collect(),
                "year" => e.year = parse_year(&v),
                "date" if e.year.is_none() => e.year = parse_year(&v),
                "journal" | "journaltitle" | "booktitle" if e.journal.is_empty() => e.journal = clean_latex(&v),
                "doi" => e.doi = v.trim().trim_start_matches("https://doi.org/").to_string(),
                "abstract" => e.abstract_text = clean_latex(&v),
                "keywords" => e.keywords = split_kw(&clean_latex(&v)),
                "file" => e.files = parse_file_field(&v),
                _ => {
                    e.extra.insert(k, clean_latex(&v));
                }
            }
        }
        out.push(e);
    }
    out
}

pub fn parse_ris(src: &str) -> Vec<BibEntry> {
    let mut out = Vec::new();
    let mut cur: Option<BibEntry> = None;
    for line in src.lines() {
        let line = line.trim_end_matches('\r');
        if line.len() < 2 {
            continue;
        }
        let (tag, val) = match line.find("  -") {
            Some(p) if p <= 4 => (line[..p].trim(), line[p + 3..].trim()),
            _ => {
                // 이어지는 줄 (주로 초록)
                if let Some(e) = cur.as_mut() {
                    if !e.abstract_text.is_empty() {
                        e.abstract_text.push(' ');
                        e.abstract_text.push_str(line.trim());
                    }
                }
                continue;
            }
        };
        match tag {
            "TY" => cur = Some(BibEntry { entry_type: val.to_lowercase(), ..Default::default() }),
            "ER" => {
                if let Some(e) = cur.take() {
                    out.push(e);
                }
            }
            _ => {
                let Some(e) = cur.as_mut() else { continue };
                match tag {
                    "TI" | "T1" if e.title.is_empty() => e.title = val.to_string(),
                    "AU" | "A1" | "A2" => e.authors.push(val.to_string()),
                    "PY" | "Y1" | "DA" if e.year.is_none() => e.year = parse_year(val),
                    "JO" | "JF" | "T2" | "JA" if e.journal.is_empty() => e.journal = val.to_string(),
                    "DO" => e.doi = val.trim_start_matches("https://doi.org/").to_string(),
                    "AB" | "N2" if e.abstract_text.is_empty() => e.abstract_text = val.to_string(),
                    "KW" => e.keywords.extend(split_kw(val)),
                    "L1" | "LK" if val.to_lowercase().ends_with(".pdf") => e.files.push(val.trim_start_matches("file://").to_string()),
                    "ID" => e.key = val.to_string(),
                    _ => {
                        e.extra.insert(tag.to_string(), val.to_string());
                    }
                }
            }
        }
    }
    if let Some(e) = cur {
        out.push(e);
    }
    out
}

fn bib_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('{', "\\{").replace('}', "\\}").replace('&', "\\&").replace('%', "\\%")
}

/// 인용 키 생성: 첫 저자 성 + 연도 + 제목 첫 단어
pub fn make_key(authors: &[String], year: Option<i32>, title: &str) -> String {
    let last = authors
        .first()
        .map(|a| if a.contains(',') { a.split(',').next().unwrap_or(a).to_string() } else { a.split_whitespace().last().unwrap_or(a).to_string() })
        .unwrap_or_else(|| "anon".into());
    let word = title.split_whitespace().find(|w| w.chars().count() > 3).unwrap_or("paper");
    let k: String = format!("{}{}{}", last, year.map(|y| y.to_string()).unwrap_or_default(), word)
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    if k.is_empty() { "ref".into() } else { k }
}

pub fn to_bibtex(e: &BibEntry) -> String {
    let mut s = format!("@{}{{{},\n", if e.entry_type.is_empty() { "article" } else { &e.entry_type }, e.key);
    let mut field = |k: &str, v: &str| {
        if !v.trim().is_empty() {
            s.push_str(&format!("  {k} = {{{}}},\n", bib_escape(v)));
        }
    };
    field("title", &e.title);
    field("author", &e.authors.join(" and "));
    field("year", &e.year.map(|y| y.to_string()).unwrap_or_default());
    field("journal", &e.journal);
    field("doi", &e.doi);
    field("keywords", &e.keywords.join(", "));
    field("abstract", &e.abstract_text);
    s.push_str("}\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bibtex_roundtrip() {
        let src = r#"
@comment{ignored {nested}}
@article{kim2024ai,
  title = {Generative {AI} and Critical Thinking},
  author = {Kim, Minji and Lee, Jun},
  year = 2024,
  journal = "Journal of {Higher} Education",
  doi = {10.1000/xyz123},
  keywords = {AI, critical thinking},
  file = {Full Text:C\:\\Users\\prof\\Zotero\\storage\\AB12\\kim.pdf:application/pdf}
}
@book{park2020, title={교육철학 입문}, author={박영희}, year={2020}}
"#;
        let es = parse_bibtex(src);
        assert_eq!(es.len(), 2);
        assert_eq!(es[0].title, "Generative AI and Critical Thinking");
        assert_eq!(es[0].authors, vec!["Kim, Minji", "Lee, Jun"]);
        assert_eq!(es[0].year, Some(2024));
        assert_eq!(es[0].journal, "Journal of Higher Education");
        assert_eq!(es[0].keywords, vec!["AI", "critical thinking"]);
        assert_eq!(es[0].files.len(), 1);
        assert!(es[0].files[0].ends_with("kim.pdf"));
        assert_eq!(es[1].title, "교육철학 입문");
        let out = to_bibtex(&es[0]);
        let back = parse_bibtex(&out);
        assert_eq!(back[0].title, es[0].title);
        assert_eq!(back[0].authors, es[0].authors);
        assert_eq!(back[0].doi, "10.1000/xyz123");
    }

    #[test]
    fn ris_parse() {
        let src = "TY  - JOUR\nTI  - AI Tutors in Seminars\nAU  - Choi, H.\nAU  - Smith, J.\nPY  - 2023/05/01\nJO  - Ed Tech Review\nDO  - 10.5555/edtr.2023\nKW  - tutoring\nKW  - seminar\nAB  - First line\ncontinued line\nER  - \n";
        let es = parse_ris(src);
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].authors.len(), 2);
        assert_eq!(es[0].year, Some(2023));
        assert_eq!(es[0].abstract_text, "First line continued line");
        assert_eq!(es[0].keywords, vec!["tutoring", "seminar"]);
    }

    #[test]
    fn key_generation() {
        assert_eq!(make_key(&["Kim, Minji".into()], Some(2024), "The Generative turn"), "Kim2024Generative");
    }
}
