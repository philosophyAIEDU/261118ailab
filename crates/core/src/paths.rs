//! 연구실(라이브러리) 폴더 구조.
//!
//! ```text
//! <연구실 폴더>/                 기본값: 문서/PhilosophyResearchHub
//!   research.db                 메타데이터·노트·검색 색인·벡터
//!   papers/                     등록한 원본 파일의 사본 (원본은 건드리지 않음)
//!   backups/                    자동(마이그레이션 전) 백업
//!   library.json                연구실 식별 정보 (형식 버전)
//! ```
//!
//! 앱 프로그램 파일과 분리되어 있으므로 업데이트·재설치·제거 시에도 유지된다.
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

pub const DB_FILE: &str = "research.db";
pub const PAPERS_DIR: &str = "papers";
pub const BACKUPS_DIR: &str = "backups";
pub const MARKER_FILE: &str = "library.json";
/// 데이터 경로 이름은 제품명 변경과 무관하게 고정한다 (하위 호환).
pub const DEFAULT_FOLDER_NAME: &str = "PhilosophyResearchHub";
pub const DEMO_FOLDER_NAME: &str = "PhilosophyResearchHub-Demo";

#[derive(Debug, Clone)]
pub struct LibraryPaths {
    pub root: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Marker {
    pub kind: String,
    pub format: u32,
    pub created_at: String,
    #[serde(default)]
    pub demo: bool,
}

impl LibraryPaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn db(&self) -> PathBuf {
        self.root.join(DB_FILE)
    }
    pub fn papers(&self) -> PathBuf {
        self.root.join(PAPERS_DIR)
    }
    pub fn backups(&self) -> PathBuf {
        self.root.join(BACKUPS_DIR)
    }
    pub fn marker(&self) -> PathBuf {
        self.root.join(MARKER_FILE)
    }

    pub fn ensure(&self, demo: bool) -> AppResult<()> {
        std::fs::create_dir_all(self.papers())?;
        std::fs::create_dir_all(self.backups())?;
        if !self.marker().exists() {
            let m = Marker { kind: "philosophy-research-hub-library".into(), format: 1, created_at: crate::now(), demo };
            std::fs::write(self.marker(), serde_json::to_vec_pretty(&m)?)?;
        }
        Ok(())
    }

    pub fn is_demo(&self) -> bool {
        std::fs::read(self.marker())
            .ok()
            .and_then(|b| serde_json::from_slice::<Marker>(&b).ok())
            .map(|m| m.demo)
            .unwrap_or(false)
    }

    /// DB에 저장된 상대 경로를 연구실 안의 절대 경로로 바꾼다.
    /// `..`·절대 경로 등으로 연구실 밖을 가리키면 거부한다 (경로 검증).
    pub fn resolve_stored(&self, rel: &str) -> AppResult<PathBuf> {
        let p = Path::new(rel);
        if p.is_absolute() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err(AppError::Blocked("허용되지 않은 파일 경로입니다".into()));
        }
        Ok(self.root.join(p))
    }
}

/// 사용자가 고른 폴더 경로가 연구실 폴더로 쓰기에 안전한지 확인한다.
pub fn validate_library_root(p: &Path) -> AppResult<()> {
    if !p.is_absolute() {
        return Err(AppError::Invalid("연구실 폴더는 전체 경로여야 합니다".into()));
    }
    if p.parent().is_none() {
        return Err(AppError::Invalid("드라이브 최상위 폴더는 연구실 폴더로 사용할 수 없습니다".into()));
    }
    if p.exists() && !p.is_dir() {
        return Err(AppError::Invalid("선택한 경로가 폴더가 아닙니다".into()));
    }
    if p.is_dir() {
        // 비어 있지 않은 폴더라면 연구실 폴더여야 한다 (다른 자료와 섞이지 않도록)
        let mut entries = std::fs::read_dir(p)?;
        let non_empty = entries.next().is_some();
        if non_empty && !p.join(MARKER_FILE).exists() && !p.join(DB_FILE).exists() {
            return Err(AppError::Invalid(
                "비어 있지 않은 일반 폴더입니다. 빈 폴더나 기존 연구실 폴더를 선택해 주세요.".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_traversal() {
        let l = LibraryPaths::new("/tmp/lib");
        assert!(l.resolve_stored("papers/a.pdf").is_ok());
        assert!(l.resolve_stored("../etc/passwd").is_err());
        assert!(l.resolve_stored("/etc/passwd").is_err());
        assert!(l.resolve_stored("papers/../../x").is_err());
    }
    #[test]
    fn validates_root() {
        let d = tempfile::tempdir().unwrap();
        assert!(validate_library_root(d.path()).is_ok());
        std::fs::write(d.path().join("other.txt"), "x").unwrap();
        assert!(validate_library_root(d.path()).is_err());
        std::fs::write(d.path().join(MARKER_FILE), "{}").unwrap();
        assert!(validate_library_root(d.path()).is_ok());
        assert!(validate_library_root(Path::new("relative/x")).is_err());
    }
}
