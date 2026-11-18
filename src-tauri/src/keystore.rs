//! Gemini API 키 보관.
//! - Windows: 자격 증명 관리자, macOS: 키체인 (운영체제가 암호화해 보관)
//! - Linux(개발·시험용): 앱 설정 폴더의 파일(소유자만 읽기 0600)
//! 키는 연구실 DB·백업·로그에 넣지 않는다.
#[allow(unused_imports)]
use prh_core::{AppError, AppResult};
use std::path::Path;

const SERVICE: &str = "kr.philosophy-ai.research-hub";
const USER: &str = "gemini-api-key";

pub fn backend() -> &'static str {
    if cfg!(any(target_os = "macos", target_os = "windows")) { "os_keychain" } else { "file" }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod imp {
    use super::*;
    fn entry() -> AppResult<keyring::Entry> {
        keyring::Entry::new(SERVICE, USER).map_err(|_| AppError::File("운영체제 보안 저장소를 열 수 없습니다".into()))
    }
    pub fn get(_: &Path) -> AppResult<Option<String>> {
        match entry()?.get_password() {
            Ok(k) => Ok(Some(k)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(AppError::File("운영체제 보안 저장소에서 키를 읽지 못했습니다".into())),
        }
    }
    pub fn set(_: &Path, key: &str) -> AppResult<()> {
        entry()?.set_password(key).map_err(|_| AppError::File("운영체제 보안 저장소에 키를 저장하지 못했습니다".into()))
    }
    pub fn delete(_: &Path) -> AppResult<()> {
        match entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(AppError::File("키를 삭제하지 못했습니다".into())),
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    use super::*;
    fn file(dir: &Path) -> std::path::PathBuf {
        dir.join("gemini.key")
    }
    pub fn get(dir: &Path) -> AppResult<Option<String>> {
        match std::fs::read_to_string(file(dir)) {
            Ok(s) if !s.trim().is_empty() => Ok(Some(s.trim().to_string())),
            _ => Ok(None),
        }
    }
    pub fn set(dir: &Path, key: &str) -> AppResult<()> {
        std::fs::create_dir_all(dir)?;
        let p = file(dir);
        std::fs::write(&p, key)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
    pub fn delete(dir: &Path) -> AppResult<()> {
        let _ = std::fs::remove_file(file(dir));
        Ok(())
    }
}

pub fn get(config_dir: &Path) -> AppResult<Option<String>> {
    let _ = (SERVICE, USER);
    imp::get(config_dir)
}
pub fn set(config_dir: &Path, key: &str) -> AppResult<()> {
    imp::set(config_dir, key)
}
pub fn delete(config_dir: &Path) -> AppResult<()> {
    imp::delete(config_dir)
}
