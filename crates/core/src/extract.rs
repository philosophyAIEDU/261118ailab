//! 파일에서 본문을 쪽(page) 단위로 추출한다.
//!
//! - PDF: 쪽 번호를 그대로 보존한다 (AI 답변의 출처 쪽 번호가 실제 PDF 쪽과 일치해야 하므로).
//! - DOCX/TXT/MD: 쪽 개념이 없으므로 page = None 인 하나의 단위로 둔다.
//! - 원본 파일은 읽기만 하고 수정하지 않는다. 손상 파일로 인한 패닉은 격리한다.
use crate::error::{AppError, AppResult};
use crate::text::normalize;
use quick_xml::events::Event;
use serde::Serialize;
use std::io::Read;
use std::path::Path;

pub const MAX_FILE_BYTES: u64 = 300 * 1024 * 1024;
const MAX_XML_PART_BYTES: u64 = 60 * 1024 * 1024;
/// 쪽당 글자 수가 이보다 적으면 "글자 없음(스캔 이미지 가능성)"으로 본다.
pub const MIN_PAGE_CHARS: usize = 25;

#[derive(Debug, Clone, Default, Serialize)]
pub struct PdfInfo {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creation_year: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Extracted {
    pub kind: String,
    /// (쪽 번호, 글). PDF는 1부터 시작하는 실제 쪽 번호, 그 외는 None.
    pub pages: Vec<(Option<u32>, String)>,
    pub page_count: Option<u32>,
    pub info: PdfInfo,
    /// 글자가 거의 없는 쪽 번호 (OCR 필요 가능성)
    pub empty_pages: Vec<u32>,
    pub warnings: Vec<String>,
}

impl Extracted {
    pub fn total_chars(&self) -> usize {
        self.pages.iter().map(|(_, t)| t.chars().count()).sum()
    }
    /// ok / partial / no_text
    pub fn status(&self) -> &'static str {
        if self.total_chars() < MIN_PAGE_CHARS {
            "no_text"
        } else if !self.empty_pages.is_empty() && self.page_count.map(|n| self.empty_pages.len() * 2 >= n as usize).unwrap_or(false) {
            "partial"
        } else {
            "ok"
        }
    }
}

pub fn document_extensions() -> &'static [&'static str] {
    &["pdf", "docx", "txt", "md", "markdown"]
}

pub fn ext_of(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

pub fn extract(path: &Path) -> AppResult<Extracted> {
    let meta = std::fs::metadata(path).map_err(|_| AppError::File("파일을 열 수 없습니다".into()))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(AppError::File("파일이 너무 큽니다 (최대 300MB)".into()));
    }
    let ext = ext_of(path);
    match ext.as_str() {
        "pdf" => extract_pdf(&std::fs::read(path)?),
        "docx" => {
            let t = extract_docx(path)?;
            Ok(single("docx", t))
        }
        "txt" | "md" | "markdown" => Ok(single(if ext == "txt" { "txt" } else { "md" }, decode_text(&std::fs::read(path)?))),
        "hwp" | "hwpx" => Err(AppError::Unsupported("한글(HWP) 문서는 아직 지원하지 않습니다. PDF로 저장한 뒤 등록해 주세요.".into())),
        "doc" => Err(AppError::Unsupported("옛 Word 형식(.doc)은 지원하지 않습니다. .docx 또는 PDF로 저장해 주세요.".into())),
        _ => Err(AppError::Unsupported(format!("지원하지 않는 파일 형식입니다 (.{ext})"))),
    }
}

fn single(kind: &str, text: String) -> Extracted {
    let t = normalize(&text);
    let mut warnings = Vec::new();
    if t.chars().count() < MIN_PAGE_CHARS {
        warnings.push("문서에서 글자를 거의 찾지 못했습니다.".into());
    }
    Extracted { kind: kind.into(), pages: vec![(None, t)], page_count: None, info: PdfInfo::default(), empty_pages: vec![], warnings }
}

pub fn extract_pdf(bytes: &[u8]) -> AppResult<Extracted> {
    let doc = std::panic::catch_unwind(|| lopdf::Document::load_mem(bytes))
        .map_err(|_| AppError::File("이 PDF는 읽을 수 없습니다 (손상된 파일일 수 있습니다)".into()))?
        .map_err(|e| {
            let m = e.to_string().to_lowercase();
            if m.contains("encrypt") || m.contains("password") {
                AppError::File("암호가 걸린 PDF입니다. 암호를 해제한 뒤 등록해 주세요.".into())
            } else {
                AppError::File("이 PDF는 읽을 수 없습니다 (손상되었거나 지원하지 않는 형식)".into())
            }
        })?;
    if doc.is_encrypted() {
        return Err(AppError::File("암호가 걸린 PDF입니다. 암호를 해제한 뒤 등록해 주세요.".into()));
    }
    let page_count = doc.get_pages().len() as u32;
    let info = pdf_info(&doc);
    let mut warnings = Vec::new();
    let pages_text = match std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem_by_pages(bytes)) {
        Ok(Ok(p)) => p,
        _ => {
            // pdf-extract가 실패하면 lopdf의 단순 추출로 쪽마다 다시 시도한다.
            warnings.push("일부 글꼴을 해석하지 못해 간이 방식으로 글자를 추출했습니다.".into());
            (1..=page_count)
                .map(|n| std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| doc.extract_text(&[n]).unwrap_or_default())).unwrap_or_default())
                .collect()
        }
    };
    let mut pages = Vec::new();
    let mut empty_pages = Vec::new();
    for (i, t) in pages_text.into_iter().enumerate() {
        let n = i as u32 + 1;
        let t = normalize(&fix_ligatures(&t));
        if t.chars().filter(|c| c.is_alphanumeric()).count() < MIN_PAGE_CHARS {
            empty_pages.push(n);
        }
        pages.push((Some(n), t));
    }
    if pages.is_empty() && page_count > 0 {
        empty_pages = (1..=page_count).collect();
    }
    if !empty_pages.is_empty() {
        if empty_pages.len() as u32 >= page_count.max(1) {
            warnings.push("PDF에서 글자를 찾지 못했습니다. 스캔한 이미지 PDF로 보입니다. OCR(문자 인식) 처리 후 다시 등록하면 검색할 수 있습니다.".into());
        } else if empty_pages.len() * 2 >= page_count as usize {
            warnings.push(format!("{}개 쪽에서 글자를 찾지 못했습니다. 일부가 스캔 이미지일 수 있습니다.", empty_pages.len()));
        }
    }
    Ok(Extracted { kind: "pdf".into(), pages, page_count: Some(page_count), info, empty_pages, warnings })
}

fn fix_ligatures(s: &str) -> String {
    s.replace('\u{FB01}', "fi").replace('\u{FB02}', "fl").replace('\u{FB00}', "ff").replace('\u{FB03}', "ffi").replace('\u{FB04}', "ffl")
}

fn pdf_info(doc: &lopdf::Document) -> PdfInfo {
    let mut info = PdfInfo::default();
    let Ok(obj) = doc.trailer.get(b"Info") else { return info };
    let dict = match obj {
        lopdf::Object::Reference(r) => match doc.get_object(*r).and_then(|o| o.as_dict()) {
            Ok(d) => d,
            Err(_) => return info,
        },
        lopdf::Object::Dictionary(d) => d,
        _ => return info,
    };
    let get = |k: &[u8]| -> Option<String> {
        let o = dict.get(k).ok()?;
        let o = match o {
            lopdf::Object::Reference(r) => doc.get_object(*r).ok()?,
            o => o,
        };
        match o {
            lopdf::Object::String(b, _) => {
                let s = decode_pdf_string(b).trim().to_string();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            }
            _ => None,
        }
    };
    info.title = get(b"Title");
    info.author = get(b"Author");
    info.subject = get(b"Subject");
    info.keywords = get(b"Keywords");
    info.creation_year = get(b"CreationDate").and_then(|d| {
        let d = d.trim_start_matches("D:");
        d.get(0..4)?.parse::<i32>().ok().filter(|y| (1900..=2100).contains(y))
    });
    info
}

pub fn decode_pdf_string(b: &[u8]) -> String {
    if b.starts_with(&[0xFE, 0xFF]) {
        let u: Vec<u16> = b[2..].chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else if b.starts_with(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(&b[3..]).into_owned()
    } else {
        match std::str::from_utf8(b) {
            Ok(s) => s.to_string(),
            Err(_) => b.iter().map(|&c| c as char).collect(),
        }
    }
}

/// UTF-8(BOM 포함) → UTF-16 → EUC-KR(CP949) 순으로 해석한다.
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some(b) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(b).into_owned();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return encoding_rs::UTF_16LE.decode(bytes).0.into_owned();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return encoding_rs::UTF_16BE.decode(bytes).0.into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::EUC_KR.decode(bytes).0.into_owned(),
    }
}

fn extract_docx(path: &Path) -> AppResult<String> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(path)?)
        .map_err(|_| AppError::File("Word 문서가 손상되었거나 형식이 올바르지 않습니다".into()))?;
    let mut xml = String::new();
    {
        let entry = z.by_name("word/document.xml").map_err(|_| AppError::File("Word 문서 본문을 찾을 수 없습니다".into()))?;
        entry.take(MAX_XML_PART_BYTES).read_to_string(&mut xml).map_err(|_| AppError::File("Word 문서를 읽지 못했습니다".into()))?;
    }
    // quick-xml은 외부 엔티티를 불러오지 않는다 (XXE 안전)
    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut out = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.name().as_ref() == b"w:t" => in_text = true,
            Ok(Event::End(e)) if e.name().as_ref() == b"w:t" => in_text = false,
            Ok(Event::End(e)) if e.name().as_ref() == b"w:p" => out.push_str("\n\n"),
            Ok(Event::End(e)) if e.name().as_ref() == b"w:tc" => out.push_str(" | "),
            Ok(Event::Empty(e)) if e.name().as_ref() == b"w:tab" => out.push('\t'),
            Ok(Event::Text(t)) if in_text => out.push_str(&t.unescape().map(|c| c.into_owned()).unwrap_or_default()),
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// 테스트용 PDF 생성 (쪽마다 지정한 영문 글을 Helvetica로 쓴다)
    pub fn make_pdf(pages: &[&str], title: Option<&str>) -> Vec<u8> {
        use lopdf::content::{Content, Operation};
        use lopdf::{dictionary, Object, Stream};
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica"});
        let resources_id = doc.add_object(dictionary! {"Font" => dictionary! {"F1" => font_id}});
        let mut kids = vec![];
        for text in pages {
            let mut ops = vec![Operation::new("BT", vec![]), Operation::new("Tf", vec!["F1".into(), 11.into()]), Operation::new("TL", vec![14.into()]), Operation::new("Td", vec![50.into(), 780.into()])];
            for line in text.split('\n') {
                ops.push(Operation::new("Tj", vec![Object::string_literal(line)]));
                ops.push(Operation::new("T*", vec![]));
            }
            ops.push(Operation::new("ET", vec![]));
            let content = Content { operations: ops };
            let cid = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            let pid = doc.add_object(dictionary! {"Type" => "Page", "Parent" => pages_id, "Contents" => cid, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()]});
            kids.push(pid.into());
        }
        let n = kids.len() as i64;
        doc.objects.insert(pages_id, Object::Dictionary(dictionary! {"Type" => "Pages", "Kids" => kids, "Count" => n, "Resources" => resources_id}));
        let catalog_id = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages_id});
        doc.trailer.set("Root", catalog_id);
        if let Some(t) = title {
            let info = doc.add_object(dictionary! {"Title" => Object::string_literal(t), "Author" => Object::string_literal("Jane Doe; John Roe"), "CreationDate" => Object::string_literal("D:20230105120000")});
            doc.trailer.set("Info", info);
        }
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn pdf_pages_are_preserved() {
        let bytes = make_pdf(&["Generative AI and critical thinking in university students", "Method: survey of 240 undergraduate students was conducted", "Results show significant improvement in reflective judgment"], Some("Test Paper"));
        let e = extract_pdf(&bytes).unwrap();
        assert_eq!(e.page_count, Some(3));
        assert_eq!(e.pages.len(), 3);
        assert!(e.pages[1].1.contains("240 undergraduate"));
        assert_eq!(e.pages[2].0, Some(3));
        assert_eq!(e.info.title.as_deref(), Some("Test Paper"));
        assert_eq!(e.info.creation_year, Some(2023));
        assert_eq!(e.status(), "ok");
    }

    #[test]
    fn blank_pdf_is_flagged_for_ocr() {
        let bytes = make_pdf(&["", ""], None);
        let e = extract_pdf(&bytes).unwrap();
        assert_eq!(e.status(), "no_text");
        assert_eq!(e.empty_pages, vec![1, 2]);
        assert!(e.warnings.iter().any(|w| w.contains("OCR")));
    }

    #[test]
    fn corrupt_pdf_is_error_not_panic() {
        assert!(extract_pdf(b"%PDF-1.4 garbage").is_err());
    }

    #[test]
    fn text_encodings() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.txt");
        std::fs::write(&p, encoding_rs::EUC_KR.encode("연구 노트: 비판적 사고").0).unwrap();
        assert_eq!(extract(&p).unwrap().pages[0].1, "연구 노트: 비판적 사고");
        let p = d.path().join("a.hwp");
        std::fs::write(&p, b"x").unwrap();
        assert!(matches!(extract(&p), Err(AppError::Unsupported(_))));
    }
}
