//! 텍스트 정규화, 검색용 토큰화, 청크 분할.
//!
//! SQLite FTS5의 기본 토크나이저는 띄어쓰기 단위로 나누므로 "비판적사고는" 같은
//! 한국어 어절에서 "비판적"을 찾지 못한다. 그래서 한글·한자·가나 구간은 2글자(bigram)
//! 토큰으로 미리 분해해 색인하고, 질의도 같은 방식으로 분해해 연속 구(phrase)로 검색한다.

pub fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0xAC00..=0xD7A3 | 0x1100..=0x11FF | 0x3130..=0x318F | // 한글
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF | // 한자
        0x3040..=0x30FF)                                      // 가나
}

/// 색인·검색용 토큰 목록. 라틴 문자 단어는 소문자 단어로, CJK 구간은 bigram으로.
pub fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut cjk: Vec<char> = Vec::new();
    let flush_word = |w: &mut String, out: &mut Vec<String>| {
        if !w.is_empty() {
            out.push(w.to_lowercase());
            w.clear();
        }
    };
    let flush_cjk = |c: &mut Vec<char>, out: &mut Vec<String>| {
        if c.len() == 1 {
            out.push(c[0].to_string());
        } else {
            for w in c.windows(2) {
                out.push(w.iter().collect());
            }
        }
        c.clear();
    };
    for ch in s.chars() {
        if is_cjk(ch) {
            flush_word(&mut word, &mut out);
            cjk.push(ch);
        } else if ch.is_alphanumeric() {
            flush_cjk(&mut cjk, &mut out);
            word.push(ch);
        } else {
            flush_word(&mut word, &mut out);
            flush_cjk(&mut cjk, &mut out);
        }
    }
    flush_word(&mut word, &mut out);
    flush_cjk(&mut cjk, &mut out);
    out
}

pub fn index_string(s: &str) -> String {
    tokens(s).join(" ")
}

const STOPWORDS: &[&str] = &[
    "the", "a", "an", "of", "and", "or", "in", "on", "for", "to", "with", "is", "are", "was", "were", "be", "by", "as",
    "at", "that", "this", "what", "which", "how", "do", "does", "about", "from", "my", "me", "i",
    "내가", "저장한", "논문", "논문중", "중", "찾아줘", "찾아", "알려줘", "알려", "주세요", "줘", "관한", "대한", "다룬", "관련",
    "관련된", "연구를", "연구", "무엇", "무엇인가", "어떤", "있는", "있나요", "인가요", "및", "그리고", "에서", "의", "를", "을",
    "은", "는", "이", "가", "와", "과", "등", "것", "수", "정리해줘", "정리", "비교해줘", "설명해줘",
];

/// 질문에서 검색어(어절)를 뽑는다. 한국어 조사는 흔한 것만 떼어 낸다.
pub fn query_terms(q: &str) -> Vec<String> {
    const SUFFIXES: &[&str] = &[
        "에서는", "으로는", "이라는", "에서의", "와의", "과의", "에게", "에서", "으로", "이란", "라는", "와", "과", "의", "를", "을",
        "은", "는", "이", "가", "에", "도", "로",
    ];
    let mut out: Vec<String> = Vec::new();
    for raw in q.split(|c: char| !(c.is_alphanumeric() || is_cjk(c))) {
        if raw.is_empty() {
            continue;
        }
        let mut w = raw.to_lowercase();
        if STOPWORDS.contains(&w.as_str()) {
            continue;
        }
        if w.chars().next().map(is_cjk).unwrap_or(false) && w.chars().count() > 2 {
            for s in SUFFIXES {
                if w.ends_with(s) && w.chars().count() > s.chars().count() + 1 {
                    w.truncate(w.len() - s.len());
                    break;
                }
            }
        }
        if STOPWORDS.contains(&w.as_str()) || (w.chars().count() < 2 && !w.chars().all(|c| c.is_ascii_digit())) {
            continue;
        }
        if !out.contains(&w) {
            out.push(w);
        }
    }
    out
}

/// FTS5 MATCH 식. 각 검색어를 토큰 구(phrase)로 만들고 OR로 잇는다. 사용자 입력은 모두 따옴표 안에 넣어
/// FTS 문법 주입을 막는다.
pub fn fts_query(terms: &[String]) -> Option<String> {
    let parts: Vec<String> = terms
        .iter()
        .filter_map(|t| {
            let toks = tokens(t);
            if toks.is_empty() {
                None
            } else {
                Some(format!("\"{}\"", toks.join(" ").replace('"', "")))
            }
        })
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" OR "))
    }
}

pub fn normalize(s: &str) -> String {
    let s = s.replace('\r', "").replace('\u{0}', "").replace('\u{ad}', "");
    let mut out = String::with_capacity(s.len());
    let mut blank = 0;
    for line in s.lines() {
        let t = line.trim_end();
        if t.trim().is_empty() {
            blank += 1;
            if blank <= 1 {
                out.push('\n');
            }
        } else {
            blank = 0;
            out.push_str(t);
            out.push('\n');
        }
    }
    out.trim().to_string()
}

/// 한 쪽(page)의 글을 검색 단위로 나눈다. 문단 경계를 우선하고, 너무 긴 문단은 문장 경계에서 자른다.
pub fn chunk_page(text: &str, target: usize, max: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut cur = String::new();
    let paras: Vec<String> = text
        .split("\n\n")
        .map(|p| p.split('\n').map(str::trim).collect::<Vec<_>>().join(" ").trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    let push = |cur: &mut String, chunks: &mut Vec<String>| {
        let t = cur.trim();
        if !t.is_empty() {
            chunks.push(t.to_string());
        }
        cur.clear();
    };
    for p in paras {
        if p.chars().count() > max {
            push(&mut cur, &mut chunks);
            let mut piece = String::new();
            for sent in split_sentences(&p) {
                if piece.chars().count() + sent.chars().count() > target && !piece.is_empty() {
                    push(&mut piece, &mut chunks);
                }
                if sent.chars().count() > max {
                    // 문장 부호가 없는 아주 긴 글: 글자 수로 자른다
                    let cs: Vec<char> = sent.chars().collect();
                    for part in cs.chunks(target) {
                        chunks.push(part.iter().collect::<String>().trim().to_string());
                    }
                    continue;
                }
                piece.push_str(&sent);
                piece.push(' ');
            }
            push(&mut piece, &mut chunks);
            continue;
        }
        if cur.chars().count() + p.chars().count() > target && !cur.is_empty() {
            push(&mut cur, &mut chunks);
        }
        cur.push_str(&p);
        cur.push('\n');
    }
    push(&mut cur, &mut chunks);
    chunks.retain(|c| c.chars().filter(|c| c.is_alphanumeric()).count() >= 8);
    chunks
}

pub fn split_sentences(p: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = p.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        cur.push(c);
        let end = matches!(c, '.' | '?' | '!' | '。') && chars.get(i + 1).map(|n| n.is_whitespace()).unwrap_or(true);
        if end {
            out.push(std::mem::take(&mut cur).trim().to_string());
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// 두 글의 어휘 겹침 비율 (a의 토큰 중 b에 있는 비율). 인용 검증에 사용.
pub fn overlap_ratio(a: &str, b: &str) -> f64 {
    let ta: Vec<String> = tokens(a).into_iter().filter(|t| !STOPWORDS.contains(&t.as_str())).collect();
    if ta.is_empty() {
        return 0.0;
    }
    let tb: std::collections::HashSet<String> = tokens(b).into_iter().collect();
    let hit = ta.iter().filter(|t| tb.contains(*t)).count();
    hit as f64 / ta.len() as f64
}

/// 짧은 미리보기 (글자 수 기준)
pub fn preview(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{t}…")
    } else {
        t
    }
}

/// 제목 비교용 정규화 (중복 검사·인용 탐지)
pub fn norm_title(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hangul_bigrams_and_words() {
        assert_eq!(tokens("비판적 사고"), vec!["비판", "판적", "사고"]);
        assert_eq!(tokens("ChatGPT와 AI"), vec!["chatgpt", "와", "ai"]);
        assert_eq!(tokens("생성형AI"), vec!["생성", "성형", "ai"]);
    }

    #[test]
    fn query_terms_strip_particles_and_stopwords() {
        let t = query_terms("내가 저장한 논문 중 생성형 AI와 비판적 사고의 관계를 다룬 연구를 찾아줘.");
        assert!(t.contains(&"생성형".to_string()));
        assert!(t.contains(&"ai".to_string()) || t.contains(&"ai와".to_string()));
        assert!(t.contains(&"비판적".to_string()));
        assert!(t.contains(&"사고".to_string()));
        assert!(t.contains(&"관계".to_string()));
        assert!(!t.contains(&"찾아줘".to_string()));
    }

    #[test]
    fn fts_query_is_quoted() {
        let q = fts_query(&["a\"b OR c".into(), "비판적".into()]).unwrap();
        assert_eq!(q, "\"a b or c\" OR \"비판 판적\"");
    }

    #[test]
    fn chunking_respects_size() {
        let para = "가나다라마바사아자차. ".repeat(200);
        let ch = chunk_page(&para, 500, 900);
        assert!(ch.len() > 2);
        assert!(ch.iter().all(|c| c.chars().count() <= 900));
        assert!(chunk_page("  \n\n ", 500, 900).is_empty());
    }

    #[test]
    fn overlap() {
        assert!(overlap_ratio("비판적 사고가 향상되었다", "실험 결과 비판적 사고가 유의하게 향상되었다") > 0.7);
        assert!(overlap_ratio("양자역학 실험", "비판적 사고") < 0.2);
    }
}
