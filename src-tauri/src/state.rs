//! 앱 상태: 현재 열린 연구실, 벡터 색인 캐시, 작업 취소 플래그, 연구실 위치 설정.
use prh_core::paths::LibraryPaths;
use prh_core::search::VectorIndex;
use prh_core::{AppError, AppResult};
use prh_core::rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};

pub struct Library {
    pub paths: LibraryPaths,
    pub db: Mutex<Connection>,
    pub index: RwLock<Option<Arc<VectorIndex>>>,
    pub open_report: prh_core::db::OpenReport,
}

impl Library {
    pub fn open(root: &Path, demo: bool) -> AppResult<Self> {
        prh_core::paths::validate_library_root(root)?;
        let paths = LibraryPaths::new(root);
        paths.ensure(demo)?;
        let (conn, report) = prh_core::db::open(&paths)?;
        Ok(Self { paths, db: Mutex::new(conn), index: RwLock::new(None), open_report: report })
    }

    pub fn conn(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        self.db.lock().map_err(|_| AppError::Db("데이터베이스 잠금 오류 — 프로그램을 다시 시작해 주세요".into()))
    }

    /// 임베딩 모델의 벡터 색인을 (변경되었을 때만) 다시 읽는다
    pub fn index_for(&self, model: &str) -> AppResult<Option<Arc<VectorIndex>>> {
        let rev = prh_core::search::embedding_revision(&*self.conn()?, model)?;
        if let Some(i) = self.index.read().unwrap().as_ref() {
            if i.model == model && i.revision == rev {
                return Ok(Some(i.clone()));
            }
        }
        let idx = { prh_core::search::load_index(&*self.conn()?, model)? };
        if idx.ids.is_empty() {
            *self.index.write().unwrap() = None;
            return Ok(None);
        }
        let a = Arc::new(idx);
        *self.index.write().unwrap() = Some(a.clone());
        Ok(Some(a))
    }
}

/// 앱 설정 폴더에 저장하는 연구실 위치 (연구실 자료와 분리)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Location {
    pub library: Option<PathBuf>,
    #[serde(default)]
    pub recent: Vec<PathBuf>,
}

pub struct AppState {
    pub lib: RwLock<Option<Arc<Library>>>,
    pub config_dir: PathBuf,
    pub default_root: PathBuf,
    pub demo_root: PathBuf,
    pub jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub pending_update: tokio::sync::Mutex<Option<tauri_plugin_updater::Update>>,
    pub open_error: RwLock<Option<String>>,
}

impl AppState {
    pub fn location_file(&self) -> PathBuf {
        self.config_dir.join("location.json")
    }
    pub fn load_location(&self) -> Location {
        std::fs::read(self.location_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub fn save_location(&self, l: &Location) -> AppResult<()> {
        std::fs::create_dir_all(&self.config_dir)?;
        let tmp = self.location_file().with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(l)?)?;
        std::fs::rename(tmp, self.location_file())?;
        Ok(())
    }
    pub fn lib(&self) -> AppResult<Arc<Library>> {
        self.lib.read().unwrap().clone().ok_or_else(|| {
            AppError::Invalid(self.open_error.read().unwrap().clone().unwrap_or_else(|| "연구실이 열려 있지 않습니다. 설정에서 연구실 폴더를 선택해 주세요.".into()))
        })
    }
    pub fn job(&self, id: &str) -> Arc<AtomicBool> {
        let f = Arc::new(AtomicBool::new(false));
        self.jobs.lock().unwrap().insert(id.to_string(), f.clone());
        f
    }
    pub fn end_job(&self, id: &str) {
        self.jobs.lock().unwrap().remove(id);
    }
    pub fn switch_to(&self, root: &Path, demo: bool) -> AppResult<Arc<Library>> {
        let lib = Arc::new(Library::open(root, demo)?);
        *self.lib.write().unwrap() = Some(lib.clone());
        *self.open_error.write().unwrap() = None;
        let mut loc = self.load_location();
        if !demo {
            loc.library = Some(root.to_path_buf());
        }
        loc.recent.retain(|p| p != root);
        loc.recent.insert(0, root.to_path_buf());
        loc.recent.truncate(8);
        self.save_location(&loc)?;
        Ok(lib)
    }
}
