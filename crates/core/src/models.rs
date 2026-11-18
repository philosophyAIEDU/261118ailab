//! 컴퓨터 사양 확인과 역할별 AI 모델 추천.
//!
//! Philosophy Local AI(261110philosophylocalai)의 사양 진단·추천 방식을 간소화해 가져왔다.
//! 추천은 메모리 예산에 따른 **추정**이며 실측 성능이 아니다.
use serde::Serialize;
use sysinfo::System;

pub const GB: f64 = 1024.0 * 1024.0 * 1024.0;

#[derive(Debug, Clone, Serialize)]
pub struct Hardware {
    pub os: String,
    pub arch: String,
    pub cpu: String,
    pub cores: usize,
    pub total_ram_gb: f64,
    pub available_ram_gb: f64,
    pub apple_silicon: bool,
}

pub fn hardware() -> Hardware {
    let mut s = System::new();
    s.refresh_memory();
    s.refresh_cpu_all();
    let cpu = s.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default();
    let arch = std::env::consts::ARCH.to_string();
    Hardware {
        os: System::long_os_version().unwrap_or_else(|| std::env::consts::OS.to_string()),
        apple_silicon: cfg!(target_os = "macos") && arch == "aarch64",
        arch,
        cpu,
        cores: s.cpus().len(),
        total_ram_gb: (s.total_memory() as f64 / GB * 10.0).round() / 10.0,
        available_ram_gb: (s.available_memory() as f64 / GB * 10.0).round() / 10.0,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogModel {
    pub name: &'static str,
    /// chat / embed
    pub role: &'static str,
    pub label: &'static str,
    pub download_gb: f64,
    pub ram_gb: f64,
    pub license: &'static str,
    pub description: &'static str,
}

/// 모델 이름은 Ollama 공식 라이브러리 기준. 용량은 기본(4비트 양자화) 태그의 근사값.
pub const CATALOG: &[CatalogModel] = &[
    CatalogModel { name: "gemma3:1b", role: "chat", label: "초경량", download_gb: 0.8, ram_gb: 2.0, license: "Gemma Terms of Use", description: "메모리 8GB 미만 컴퓨터용. 짧은 요약은 가능하지만 긴 근거 종합 품질은 제한적입니다." },
    CatalogModel { name: "gemma3:4b", role: "chat", label: "가벼움", download_gb: 3.3, ram_gb: 4.0, license: "Gemma Terms of Use", description: "빠른 응답. 한국어 질의응답과 짧은 비교 정리에 적합합니다." },
    CatalogModel { name: "qwen3:8b", role: "chat", label: "균형 (추천)", download_gb: 5.2, ram_gb: 6.5, license: "Apache-2.0", description: "속도와 품질의 균형. 논문 근거 종합·비교표 작성에 적합합니다." },
    CatalogModel { name: "gemma3:12b", role: "chat", label: "고품질", download_gb: 8.1, ram_gb: 12.0, license: "Gemma Terms of Use", description: "더 정확한 종합과 문헌고찰 초안. 메모리 16GB 이상 권장." },
    CatalogModel { name: "qwen3:14b", role: "chat", label: "고품질", download_gb: 9.3, ram_gb: 13.0, license: "Apache-2.0", description: "복잡한 비교·추론에 강함. 메모리 24GB 이상 권장." },
    CatalogModel { name: "bge-m3", role: "embed", label: "다국어 임베딩 (추천)", download_gb: 1.2, ram_gb: 2.0, license: "MIT", description: "한국어·영어 논문의 의미 검색에 적합한 다국어 임베딩 모델입니다." },
    CatalogModel { name: "nomic-embed-text", role: "embed", label: "가벼운 임베딩 (영어 중심)", download_gb: 0.3, ram_gb: 1.0, license: "Apache-2.0", description: "가볍고 빠르지만 한국어 검색 품질은 낮을 수 있습니다." },
];

#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    pub chat: &'static str,
    pub chat_light: &'static str,
    pub embed: &'static str,
    pub reasons: Vec<String>,
}

/// 메모리 예산: Apple Silicon은 통합 메모리의 약 65%, 그 외는 약 60%를 AI에 쓸 수 있다고 가정.
pub fn recommend(hw: &Hardware) -> Recommendation {
    let budget = hw.total_ram_gb * if hw.apple_silicon { 0.65 } else { 0.6 };
    let embed_ram = 2.0;
    let fits: Vec<&CatalogModel> = CATALOG.iter().filter(|m| m.role == "chat" && m.ram_gb + embed_ram <= budget).collect();
    let chat = if budget >= 8.5 && fits.iter().any(|m| m.name == "qwen3:8b") {
        "qwen3:8b"
    } else {
        fits.iter().rev().find(|m| m.name != "qwen3:14b" && m.name != "gemma3:12b").map(|m| m.name).unwrap_or("gemma3:1b")
    };
    let chat_light = if budget >= 5.5 { "gemma3:4b" } else { "gemma3:1b" };
    let embed = if budget >= 4.0 { "bge-m3" } else { "nomic-embed-text" };
    let mut reasons = vec![format!("전체 메모리 {:.0}GB 중 AI가 쓸 수 있는 메모리를 약 {:.0}GB로 추정했습니다.", hw.total_ram_gb, budget)];
    if hw.apple_silicon {
        reasons.push("Apple 칩의 통합 메모리와 그래픽 가속을 사용할 수 있습니다.".into());
    }
    if budget < 7.0 {
        reasons.push("메모리가 넉넉하지 않아 가벼운 모델을 추천합니다. 긴 문헌 종합은 품질이 제한될 수 있습니다.".into());
    }
    reasons.push("이 추천은 사양 기반 추정이며, 실제 속도는 다른 프로그램 사용량에 따라 달라집니다.".into());
    Recommendation { chat, chat_light, embed, reasons }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hw(ram: f64, apple: bool) -> Hardware {
        Hardware { os: "t".into(), arch: "x".into(), cpu: "c".into(), cores: 8, total_ram_gb: ram, available_ram_gb: ram / 2.0, apple_silicon: apple }
    }
    #[test]
    fn recommendations_scale_with_memory() {
        assert_eq!(recommend(&hw(8.0, false)).chat, "gemma3:1b");
        assert_eq!(recommend(&hw(16.0, false)).chat, "qwen3:8b");
        assert_eq!(recommend(&hw(16.0, true)).embed, "bge-m3");
        assert_eq!(recommend(&hw(4.0, false)).embed, "nomic-embed-text");
        assert!(hardware().total_ram_gb > 0.0);
    }
}
