//! 화면(React)에서 호출하는 명령. 모든 처리는 이 컴퓨터 안에서 이루어진다.
use crate::state::{AppState, Library};
use prh_core::export::Format;
use prh_core::*;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

type R<T> = Result<T, AppError>;

#[derive(Serialize, Clone)]
pub struct JobEvent {
    pub id: String,
    pub kind: String,
    /// progress / token / done / error
    pub stage: String,
    pub done: usize,
    pub total: usize,
    pub message: String,
    pub data: Value,
}

pub fn emit(app: &AppHandle, ev: JobEvent) {
    let _ = app.emit("job", ev);
}

fn ev(id: &str, kind: &str, stage: &str, done: usize, total: usize, message: impl Into<String>, data: Value) -> JobEvent {
    JobEvent { id: id.into(), kind: kind.into(), stage: stage.into(), done, total, message: message.into(), data }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> R<T> + Send + 'static) -> R<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|_| AppError::Invalid("작업 실행 중 오류가 발생했습니다".into()))?
}

// ───────────────────────── 앱·연구실 ─────────────────────────

#[tauri::command]
pub async fn app_info(app: AppHandle, state: State<'_, AppState>) -> R<Value> {
    let lib = state.lib.read().unwrap().clone();
    Ok(json!({
        "product_ko": "교수님만의 AI 연구실",
        "product_en": "Philosophy Research Hub",
        "publisher": "필로소피 AI 교육",
        "version": app.package_info().version.to_string(),
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "schema_version": db::latest_version(),
        "library": lib.as_ref().map(|l| json!({
            "root": l.paths.root.to_string_lossy(),
            "demo": l.paths.is_demo(),
            "migrated_from": l.open_report.from_version,
            "migration_backup": l.open_report.backup_path.as_ref().map(|p| p.to_string_lossy().to_string()),
        })),
        "open_error": state.open_error.read().unwrap().clone(),
        "default_root": state.default_root.to_string_lossy(),
        "demo_root": state.demo_root.to_string_lossy(),
        "recent": state.load_location().recent,
        "config_dir": state.config_dir.to_string_lossy(),
    }))
}

#[tauri::command]
pub async fn library_open(state: State<'_, AppState>, path: String) -> R<Value> {
    let p = PathBuf::from(&path);
    let lib = state.switch_to(&p, false)?;
    Ok(json!({"root": lib.paths.root, "demo": lib.paths.is_demo()}))
}

#[tauri::command]
pub async fn library_open_default(state: State<'_, AppState>) -> R<Value> {
    let root = state.default_root.clone();
    let lib = state.switch_to(&root, false)?;
    Ok(json!({"root": lib.paths.root}))
}

/// 데모 연구실을 만든다(또는 연다). 실제 연구실과 다른 폴더를 사용한다.
#[tauri::command]
pub async fn demo_open(app: AppHandle, state: State<'_, AppState>, job: String) -> R<Value> {
    let root = state.demo_root.clone();
    let fresh = !root.join(paths::DB_FILE).exists();
    let lib = state.switch_to(&root, true)?;
    if fresh {
        let res = app.path().resource_dir().map_err(|_| AppError::File("프로그램 자료 폴더를 찾을 수 없습니다".into()))?.join("demo");
        let files: Vec<PathBuf> = library::collect_files(&res).into_iter().filter(|p| extract::ext_of(p) == "pdf").collect();
        if files.is_empty() {
            return Err(AppError::NotFound("데모 자료가 설치 파일에 포함되어 있지 않습니다".into()));
        }
        let n = files.len();
        for (i, f) in files.iter().enumerate() {
            emit(&app, ev(&job, "demo", "progress", i, n, format!("가상 데모 논문 등록 중 ({}/{n})", i + 1), Value::Null));
            let l = lib.clone();
            let f = f.clone();
            blocking(move || library::import_file(&mut *l.conn()?, &l.paths, &f).map(|_| ())).await?;
        }
        let l = lib.clone();
        blocking(move || demo::seed(&mut *l.conn()?)).await?;
        emit(&app, ev(&job, "demo", "done", n, n, "데모 연구실을 준비했습니다", Value::Null));
    }
    Ok(json!({"root": lib.paths.root, "created": fresh}))
}

#[tauri::command]
pub async fn reveal_library(app: AppHandle, state: State<'_, AppState>) -> R<()> {
    use tauri_plugin_opener::OpenerExt;
    let lib = state.lib()?;
    app.opener().open_path(lib.paths.root.to_string_lossy(), None::<&str>).map_err(|_| AppError::File("폴더를 열 수 없습니다".into()))
}

/// 허용된 주소만 기본 브라우저로 연다
#[tauri::command]
pub async fn open_url(app: AppHandle, url: String) -> R<()> {
    use tauri_plugin_opener::OpenerExt;
    const ALLOWED: &[&str] = &["https://ollama.com/download", "https://ollama.com/library", "https://github.com/philosophyAIEDU/", "https://www.youtube.com/@"];
    if !ALLOWED.iter().any(|a| url.starts_with(a)) {
        return Err(AppError::Blocked("허용되지 않은 주소입니다".into()));
    }
    app.opener().open_url(url, None::<&str>).map_err(|_| AppError::File("브라우저를 열 수 없습니다".into()))
}

#[tauri::command]
pub async fn dashboard(state: State<'_, AppState>) -> R<Value> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    let stats = library::stats(&c)?;
    let recent = library::list_papers(&c, &library::PaperFilter { sort: Some("recent".into()), ..Default::default() })?.into_iter().take(6).collect::<Vec<_>>();
    let searches = rag::recent_searches(&c, 5)?;
    let projects: Vec<_> = projects::list(&c)?.into_iter().filter(|p| p.status == "active").take(5).collect();
    let s = settings::load(&c)?;
    Ok(json!({"stats": stats, "recent_papers": recent, "recent_searches": searches, "projects": projects, "settings": s, "demo": lib.paths.is_demo()}))
}

// ───────────────────────── 논문 ─────────────────────────

#[tauri::command]
pub async fn papers_list(state: State<'_, AppState>, filter: library::PaperFilter) -> R<Vec<library::Paper>> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    library::list_papers(&c, &filter)
}

#[tauri::command]
pub async fn paper_get(state: State<'_, AppState>, id: String) -> R<Value> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    let p = library::get_paper(&c, &id)?;
    let notes = notes::list(&c, &notes::NoteFilter { paper_id: Some(id.clone()), ..Default::default() })?;
    let cites: Vec<Value> = c
        .prepare("SELECT c.cited_id, p.title, c.evidence, c.page, 'out' FROM citations c JOIN papers p ON p.id=c.cited_id WHERE c.citing_id=?1 UNION ALL SELECT c.citing_id, p.title, c.evidence, c.page, 'in' FROM citations c JOIN papers p ON p.id=c.citing_id WHERE c.cited_id=?1")?
        .query_map([&id], |r| Ok(json!({"paper_id": r.get::<_, String>(0)?, "title": r.get::<_, String>(1)?, "evidence": r.get::<_, String>(2)?, "page": r.get::<_, Option<i64>>(3)?, "direction": r.get::<_, String>(4)?})))?
        .collect::<Result<_, _>>()?;
    let projects: Vec<Value> = c
        .prepare("SELECT pr.id, pr.name FROM project_papers pp JOIN projects pr ON pr.id=pp.project_id WHERE pp.paper_id=?1")?
        .query_map([&id], |r| Ok(json!({"id": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)?})))?
        .collect::<Result<_, _>>()?;
    Ok(json!({"paper": p, "notes": notes, "citations": cites, "projects": projects}))
}

#[tauri::command]
pub async fn paper_update(state: State<'_, AppState>, id: String, update: library::MetadataUpdate) -> R<library::Paper> {
    let lib = state.lib()?;
    let mut c = lib.conn()?;
    library::update_metadata(&mut c, &id, &update)
}

#[tauri::command]
pub async fn paper_delete(state: State<'_, AppState>, ids: Vec<String>) -> R<usize> {
    let lib = state.lib()?;
    let mut c = lib.conn()?;
    for id in &ids {
        library::delete_paper(&mut c, &lib.paths, id)?;
    }
    Ok(ids.len())
}

#[tauri::command]
pub async fn paper_reprocess(state: State<'_, AppState>, id: String) -> R<library::Paper> {
    let lib = state.lib()?;
    blocking(move || library::reprocess(&mut *lib.conn()?, &lib.paths, &id)).await
}

#[tauri::command]
pub async fn papers_tag(state: State<'_, AppState>, ids: Vec<String>, add: Vec<String>, remove: Vec<String>) -> R<()> {
    let lib = state.lib()?;
    let mut c = lib.conn()?;
    library::set_tags(&mut c, &ids, &add, &remove)
}

#[tauri::command]
pub async fn tags_list(state: State<'_, AppState>) -> R<Vec<(String, i64)>> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    library::all_tags(&c)
}

#[tauri::command]
pub async fn paper_chunks(state: State<'_, AppState>, id: String) -> R<Vec<library::ChunkRow>> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    library::paper_chunks(&c, &id)
}

/// PDF 뷰어용 파일 내용 (연구실 사본만 읽을 수 있음)
#[tauri::command]
pub async fn paper_file(state: State<'_, AppState>, id: String) -> R<tauri::ipc::Response> {
    let lib = state.lib()?;
    let path = { library::paper_file_abs(&*lib.conn()?, &lib.paths, &id)? };
    let bytes = blocking(move || Ok(std::fs::read(path)?)).await?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// 파일·폴더 등록 (백그라운드 작업, 진행률은 "job" 이벤트로 알림)
#[tauri::command]
pub async fn import_start(app: AppHandle, state: State<'_, AppState>, paths: Vec<String>, job: String) -> R<()> {
    let lib = state.lib()?;
    let cancel = state.job(&job);
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut files: Vec<PathBuf> = Vec::new();
        for p in paths {
            let p = PathBuf::from(p);
            if p.is_dir() {
                files.extend(library::collect_files(&p));
            } else {
                files.push(p);
            }
        }
        files.dedup();
        let total = files.len();
        let mut outcomes: Vec<library::ImportOutcome> = Vec::new();
        for (i, f) in files.into_iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let name = f.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            emit(&app2, ev(&job, "import", "progress", i, total, format!("{name} 처리 중"), Value::Null));
            let l = lib.clone();
            let ext = extract::ext_of(&f);
            let src_path = f.to_string_lossy().to_string();
            let res: R<Vec<library::ImportOutcome>> = if ext == "bib" || ext == "ris" {
                blocking(move || {
                    let src = extract::decode_text(&std::fs::read(&f)?);
                    let entries = if ext == "bib" { bib::parse_bibtex(&src) } else { bib::parse_ris(&src) };
                    library::import_bib_entries(&mut *l.conn()?, &l.paths, &entries, if ext == "bib" { "bibtex" } else { "ris" })
                })
                .await
            } else {
                let f2 = f.clone();
                blocking(move || {
                    let (hash, size) = library::hash_and_check(&f2)?;
                    if let Some((id, title)) = library::find_by_hash(&*l.conn()?, &hash)? {
                        return Ok(vec![library::ImportOutcome { file_name: f2.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), source_path: f2.to_string_lossy().to_string(), status: library::ImportStatus::Duplicate, paper_id: Some(id), message: format!("이미 등록된 파일입니다: {title}"), warnings: vec![] }]);
                    }
                    // 본문 추출은 DB 잠금 없이 수행
                    let prepared = library::prepare(&f2, hash, size);
                    Ok(vec![library::commit(&mut *l.conn()?, &l.paths, prepared)?])
                })
                .await
            };
            match res {
                Ok(v) => outcomes.extend(v),
                Err(e) => outcomes.push(library::ImportOutcome { file_name: name, source_path: src_path, status: library::ImportStatus::Failed, paper_id: None, message: e.to_string(), warnings: vec![] }),
            }
            emit(&app2, ev(&job, "import", "progress", i + 1, total, "", json!(outcomes.last())));
        }
        emit(&app2, ev(&job, "import", "done", outcomes.len(), total, "등록을 마쳤습니다", json!(outcomes)));
        app2.state::<AppState>().end_job(&job);
    });
    Ok(())
}

#[tauri::command]
pub async fn job_cancel(state: State<'_, AppState>, job: String) -> R<()> {
    if let Some(f) = state.jobs.lock().unwrap().get(&job) {
        f.store(true, Ordering::Relaxed);
    }
    Ok(())
}

// ───────────────────────── 주석 ─────────────────────────

#[tauri::command]
pub async fn annotations_list(state: State<'_, AppState>, paper_id: String) -> R<Vec<annotations::Annotation>> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    annotations::list(&c, &paper_id)
}

#[tauri::command]
pub async fn annotation_add(state: State<'_, AppState>, input: annotations::AnnotationInput) -> R<annotations::Annotation> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    annotations::add(&c, &input)
}

#[tauri::command]
pub async fn annotation_update(state: State<'_, AppState>, id: String, note: String) -> R<()> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    annotations::update_note(&c, &id, &note)
}

#[tauri::command]
pub async fn annotation_delete(state: State<'_, AppState>, id: String) -> R<()> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    annotations::delete(&c, &id)
}

// ───────────────────────── 검색·AI ─────────────────────────

fn ollama_for(s: &settings::Settings) -> R<ollama::Ollama> {
    ollama::Ollama::new(s.ollama_port)
}

#[tauri::command]
pub async fn search_passages(state: State<'_, AppState>, query: String, mode: search::Mode, limit: Option<usize>, paper_ids: Option<Vec<String>>) -> R<search::SearchOutcome> {
    let lib = state.lib()?;
    let s = settings::load(&*lib.conn()?)?;
    let ol = ollama_for(&s)?;
    let idx = match &s.embed_model {
        Some(m) if mode != search::Mode::Keyword => lib.index_for(m)?,
        _ => None,
    };
    let qvec = match (&idx, &s.embed_model) {
        (Some(_), Some(m)) => ol.embed(m, &[query.clone()]).await.ok().and_then(|v| v.into_iter().next()),
        _ => None,
    };
    let c = lib.conn()?;
    let ids = paper_ids.unwrap_or_default();
    search::search(&c, &search::SearchParams { query: &query, mode, limit: limit.unwrap_or(20).clamp(1, 50), per_paper: 3, papers: if ids.is_empty() { None } else { Some(&ids) } }, idx.as_deref(), qvec.as_deref())
}

#[tauri::command]
pub async fn ask(app: AppHandle, state: State<'_, AppState>, req: rag::AskRequest, job: String) -> R<rag::AskResult> {
    let lib = state.lib()?;
    let s = settings::load(&*lib.conn()?)?;
    let ol = ollama_for(&s)?;
    let idx = match &s.embed_model {
        Some(m) => lib.index_for(m)?,
        None => None,
    };
    let cancel = state.job(&job);
    let ai = rag::AiConfig { ollama: &ol, chat_model: s.chat_model.clone(), embed_model: s.embed_model.clone(), index: idx.as_deref() };
    let app2 = app.clone();
    let job2 = job.clone();
    let r = rag::ask(&lib.db, &ai, &req, cancel, move |t| emit(&app2, ev(&job2, "ask", "token", 0, 0, t, Value::Null))).await;
    state.end_job(&job);
    r
}

#[tauri::command]
pub async fn searches_recent(state: State<'_, AppState>, limit: Option<usize>) -> R<Vec<Value>> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    rag::recent_searches(&c, limit.unwrap_or(30))
}

#[tauri::command]
pub async fn search_delete(state: State<'_, AppState>, id: String) -> R<()> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    rag::delete_search(&c, &id)
}

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> R<settings::Settings> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    settings::load(&c)
}

#[tauri::command]
pub async fn settings_save(state: State<'_, AppState>, settings: settings::Settings) -> R<settings::Settings> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    prh_core::settings::save(&c, &settings)
}

#[tauri::command]
pub async fn ai_status(state: State<'_, AppState>) -> R<Value> {
    let lib = state.lib()?;
    let s = settings::load(&*lib.conn()?)?;
    let ol = ollama_for(&s)?;
    let health = ol.health().await;
    let (installed, running) = if health.running { (ol.list_models().await.unwrap_or_default(), ol.running().await.unwrap_or_default()) } else { (vec![], vec![]) };
    let hw = tauri::async_runtime::spawn_blocking(models::hardware).await.map_err(|_| AppError::Invalid("사양 확인 실패".into()))?;
    let rec = models::recommend(&hw);
    let (pending, total) = {
        let c = lib.conn()?;
        let total: i64 = c.query_row("SELECT count(*) FROM chunks", [], |r| r.get(0))?;
        (s.embed_model.as_ref().map(|m| search::count_pending(&c, m)).transpose()?, total)
    };
    Ok(json!({
        "health": health,
        "installed": installed,
        "running": running,
        "settings": s,
        "hardware": hw,
        "recommendation": rec,
        "catalog": models::CATALOG,
        "index": {"pending": pending, "total_chunks": total},
    }))
}

#[tauri::command]
pub async fn ai_pull(app: AppHandle, state: State<'_, AppState>, model: String, job: String) -> R<()> {
    let lib = state.lib()?;
    let s = settings::load(&*lib.conn()?)?;
    let ol = ollama_for(&s)?;
    let cancel = state.job(&job);
    let app2 = app.clone();
    let j = job.clone();
    let r = ol
        .pull(&model, cancel, move |p| {
            emit(&app2, ev(&j, "pull", "progress", p.completed as usize, p.total as usize, p.status.clone(), Value::Null));
        })
        .await;
    state.end_job(&job);
    r
}

#[tauri::command]
pub async fn ai_unload(state: State<'_, AppState>, model: String) -> R<()> {
    let lib = state.lib()?;
    let s = settings::load(&*lib.conn()?)?;
    ollama_for(&s)?.unload(&model).await
}

#[tauri::command]
pub async fn ai_delete_model(state: State<'_, AppState>, model: String) -> R<()> {
    let lib = state.lib()?;
    let s = settings::load(&*lib.conn()?)?;
    ollama_for(&s)?.delete_model(&model).await
}

/// 의미 검색 색인 만들기: 임베딩이 없는 청크를 16개씩 임베딩한다
#[tauri::command]
pub async fn index_build(app: AppHandle, state: State<'_, AppState>, job: String) -> R<usize> {
    let lib = state.lib()?;
    let s = settings::load(&*lib.conn()?)?;
    let model = s.embed_model.clone().ok_or_else(|| AppError::Invalid("임베딩 모델을 먼저 선택해 주세요".into()))?;
    let ol = ollama_for(&s)?;
    let cancel = state.job(&job);
    let total = { search::count_pending(&*lib.conn()?, &model)? as usize };
    let mut done = 0;
    let result: R<usize> = async {
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::Cancelled);
            }
            let batch = { search::pending_chunks(&*lib.conn()?, &model, 16)? };
            if batch.is_empty() {
                break;
            }
            let inputs: Vec<String> = batch.iter().map(|(_, t, x)| search::embed_input(t, x)).collect();
            let vecs = ol.embed(&model, &inputs).await?;
            let items: Vec<(i64, Vec<f32>)> = batch.iter().map(|b| b.0).zip(vecs).collect();
            search::store_embeddings(&mut *lib.conn()?, &model, &items)?;
            done += items.len();
            emit(&app, ev(&job, "index", "progress", done, total, format!("의미 검색 색인 {done}/{total}"), Value::Null));
        }
        Ok(done)
    }
    .await;
    state.end_job(&job);
    let _ = lib.index_for(&model);
    emit(&app, ev(&job, "index", if result.is_ok() { "done" } else { "error" }, done, total, "", Value::Null));
    result
}

// ───────────────────────── 비교 ─────────────────────────

#[tauri::command]
pub async fn compare_aspects() -> R<Vec<Value>> {
    Ok(compare::ASPECTS.iter().map(|a| json!({"key": a.key, "label": a.label})).collect())
}

fn chat_model(s: &settings::Settings) -> R<String> {
    s.chat_model.clone().ok_or_else(|| AppError::Invalid("답변 생성 모델을 먼저 선택해 주세요 (설정 > AI 모델)".into()))
}

#[tauri::command]
pub async fn compare_run(app: AppHandle, state: State<'_, AppState>, paper_ids: Vec<String>, aspects: Vec<String>, title: String, job: String) -> R<compare::Comparison> {
    let lib = state.lib()?;
    let s = settings::load(&*lib.conn()?)?;
    let model = chat_model(&s)?;
    let ol = ollama_for(&s)?;
    let cancel = state.job(&job);
    let app2 = app.clone();
    let j = job.clone();
    let r = compare::run(&lib.db, &ol, &model, &paper_ids, &aspects, &title, cancel, move |d, t, name| emit(&app2, ev(&j, "compare", "progress", d, t, name, Value::Null))).await;
    state.end_job(&job);
    r
}

#[tauri::command]
pub async fn compare_list(state: State<'_, AppState>) -> R<Vec<compare::Comparison>> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    compare::list(&c)
}

#[tauri::command]
pub async fn compare_delete(state: State<'_, AppState>, id: String) -> R<()> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    compare::delete(&c, &id)
}

// ───────────────────────── 노트·프로젝트 ─────────────────────────

#[tauri::command]
pub async fn notes_list(state: State<'_, AppState>, filter: notes::NoteFilter) -> R<Vec<notes::Note>> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    notes::list(&c, &filter)
}

#[tauri::command]
pub async fn note_get(state: State<'_, AppState>, id: String) -> R<notes::NoteDetail> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    notes::get(&c, &id)
}

#[tauri::command]
pub async fn note_save(state: State<'_, AppState>, id: Option<String>, input: notes::NoteInput) -> R<notes::NoteDetail> {
    let lib = state.lib()?;
    let mut c = lib.conn()?;
    match id {
        Some(id) => notes::update(&mut c, &id, &input),
        None => notes::create(&mut c, &input),
    }
}

#[tauri::command]
pub async fn note_delete(state: State<'_, AppState>, id: String) -> R<()> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    notes::delete(&c, &id)
}

/// AI 연구 노트 초안 — 저장하지 않고 돌려준다 (교수님이 검토 후 저장)
#[tauri::command]
pub async fn note_ai_draft(app: AppHandle, state: State<'_, AppState>, paper_id: String, focus: String, job: String) -> R<Value> {
    let lib = state.lib()?;
    let s = settings::load(&*lib.conn()?)?;
    let model = chat_model(&s)?;
    let ol = ollama_for(&s)?;
    let (paper, hits) = {
        let c = lib.conn()?;
        (library::get_paper(&c, &paper_id)?, compare::gather(&c, &paper_id, &compare::aspect_list(&[]), 10)?)
    };
    if hits.is_empty() {
        return Err(AppError::Invalid("이 논문에는 추출된 본문이 없어 초안을 만들 수 없습니다".into()));
    }
    let sources = rag::to_sources(&hits);
    let cancel = state.job(&job);
    let app2 = app.clone();
    let j = job.clone();
    let raw = ol.chat(&model, &notes::draft_messages(&paper.title, &sources, &focus), 0.3, Some(8192), false, cancel, move |t| emit(&app2, ev(&j, "draft", "token", 0, 0, t, Value::Null))).await;
    state.end_job(&job);
    let v = rag::verify(&raw?, &sources);
    let mut body = v.answer.clone();
    body.push_str("\n\n---\n**근거 출처**\n\n");
    for c in &v.citations {
        body.push_str(&format!("- [{}] p.{} — {}\n", c.label, c.source.page.map(|p| p.to_string()).unwrap_or("-".into()), text::preview(&c.source.text, 80)));
    }
    Ok(json!({"title": format!("{} — 연구 노트", text::preview(&paper.title, 60)), "body": body, "citations": v.citations, "removed_markers": v.removed_markers, "removed_references": v.removed_references, "paper_id": paper_id, "model": model}))
}

#[tauri::command]
pub async fn projects_list(state: State<'_, AppState>) -> R<Vec<projects::Project>> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    projects::list(&c)
}

#[tauri::command]
pub async fn project_save(state: State<'_, AppState>, id: Option<String>, input: projects::ProjectInput) -> R<projects::Project> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    match id {
        Some(id) => projects::update(&c, &id, &input),
        None => projects::create(&c, &input),
    }
}

#[tauri::command]
pub async fn project_delete(state: State<'_, AppState>, id: String) -> R<()> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    projects::delete(&c, &id)
}

#[tauri::command]
pub async fn project_papers(state: State<'_, AppState>, id: String, add: Vec<String>, remove: Vec<String>) -> R<projects::Project> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    projects::set_papers(&c, &id, &add, &remove)
}

// ───────────────────────── 지식 그래프 ─────────────────────────

#[tauri::command]
pub async fn graph_get(state: State<'_, AppState>) -> R<graph::Graph> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    graph::build(&c)
}

#[tauri::command]
pub async fn graph_suggest(state: State<'_, AppState>) -> R<usize> {
    let lib = state.lib()?;
    let s = settings::load(&*lib.conn()?)?;
    let m = s.embed_model.ok_or_else(|| AppError::Invalid("임베딩 모델을 먼저 선택하고 의미 검색 색인을 만들어 주세요".into()))?;
    let idx = lib.index_for(&m)?.ok_or_else(|| AppError::Invalid("의미 검색 색인이 없습니다. 설정 > AI 모델에서 색인을 만들어 주세요.".into()))?;
    let c = lib.conn()?;
    graph::suggest_links(&c, &idx, 0.75)
}

#[tauri::command]
pub async fn graph_link(state: State<'_, AppState>, src_type: String, src_id: String, dst_type: String, dst_id: String, label: String) -> R<()> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    graph::add_user_link(&c, &src_type, &src_id, &dst_type, &dst_id, &label)
}

#[tauri::command]
pub async fn graph_resolve(state: State<'_, AppState>, link_id: String, accept: bool) -> R<()> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    graph::resolve_suggestion(&c, &link_id, accept)
}

#[tauri::command]
pub async fn graph_unlink(state: State<'_, AppState>, link_id: String) -> R<()> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    graph::delete_link(&c, &link_id)
}

// ───────────────────────── 문헌고찰 ─────────────────────────

#[tauri::command]
pub async fn review_run(app: AppHandle, state: State<'_, AppState>, paper_ids: Vec<String>, topic: String, job: String) -> R<review::Review> {
    let lib = state.lib()?;
    let s = settings::load(&*lib.conn()?)?;
    let model = chat_model(&s)?;
    let ol = ollama_for(&s)?;
    let cancel = state.job(&job);
    let app2 = app.clone();
    let j = job.clone();
    let r = review::run(&lib.db, &ol, &model, &paper_ids, &topic, cancel, move |t| emit(&app2, ev(&j, "review", "token", 0, 0, t, Value::Null))).await;
    state.end_job(&job);
    r
}

#[tauri::command]
pub async fn review_list(state: State<'_, AppState>) -> R<Vec<review::Review>> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    review::list(&c)
}

#[tauri::command]
pub async fn review_delete(state: State<'_, AppState>, id: String) -> R<()> {
    let lib = state.lib()?;
    let c = lib.conn()?;
    review::delete(&c, &id)
}

// ───────────────────────── 내보내기 ─────────────────────────

/// 저장 경로 검증: 전체 경로, 형식과 맞는 확장자, 연구실 원본 사본 폴더가 아닐 것
fn check_out(lib: &Library, path: &str, fmt: Format) -> R<PathBuf> {
    let p = PathBuf::from(path);
    if !p.is_absolute() {
        return Err(AppError::Invalid("저장 위치가 올바르지 않습니다".into()));
    }
    let ext = extract::ext_of(&p);
    if ext != fmt.ext() {
        return Err(AppError::Invalid(format!("파일 확장자는 .{} 이어야 합니다", fmt.ext())));
    }
    if p.starts_with(lib.paths.papers()) || p == lib.paths.db() {
        return Err(AppError::Blocked("연구실 내부 자료 폴더에는 저장할 수 없습니다".into()));
    }
    Ok(p)
}

#[derive(serde::Deserialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum ExportTarget {
    Papers { ids: Option<Vec<String>> },
    Comparison { id: String },
    Notes { ids: Option<Vec<String>> },
    Review { id: String },
    Project { id: String },
    Search { id: String },
}

#[tauri::command]
pub async fn export_file(state: State<'_, AppState>, target: ExportTarget, format: Format, path: String) -> R<String> {
    let lib = state.lib()?;
    let out = check_out(&lib, &path, format)?;
    let out2 = out.clone();
    blocking(move || {
        let c = lib.conn()?;
        match target {
            ExportTarget::Papers { ids } => {
                let mut ps = library::list_papers(&c, &Default::default())?;
                if let Some(ids) = ids.filter(|v| !v.is_empty()) {
                    ps.retain(|p| ids.contains(&p.id));
                }
                export::export_papers(&ps, format, &out2)
            }
            ExportTarget::Comparison { id } => export::export_comparison(&compare::get(&c, &id)?, format, &out2),
            ExportTarget::Notes { ids } => {
                let mut ns = notes::list(&c, &Default::default())?;
                if let Some(ids) = ids.filter(|v| !v.is_empty()) {
                    ns.retain(|n| ids.contains(&n.id));
                }
                export::export_notes(&ns, &c, format, &out2)
            }
            ExportTarget::Review { id } => export::export_review(&review::get(&c, &id)?, format, &out2),
            ExportTarget::Project { id } => export::export_project(&c, &id, format, &out2),
            ExportTarget::Search { id } => {
                let v: String = c.query_row("SELECT result FROM searches WHERE id=?1", [&id], |r| r.get(0)).map_err(|_| AppError::NotFound("검색 기록을 찾을 수 없습니다".into()))?;
                let ask: rag::AskResult = serde_json::from_str(&v)?;
                export::export_ask(&ask, format, &out2)
            }
        }
    })
    .await?;
    Ok(out.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn export_obsidian(state: State<'_, AppState>, dir: String) -> R<export::ObsidianReport> {
    let lib = state.lib()?;
    let d = PathBuf::from(dir);
    if !d.is_absolute() || d.starts_with(&lib.paths.root) {
        return Err(AppError::Invalid("연구실 폴더 밖의 Obsidian 보관함 폴더를 선택해 주세요".into()));
    }
    blocking(move || export::export_obsidian(&*lib.conn()?, &d)).await
}

// ───────────────────────── 백업·복원 ─────────────────────────

#[tauri::command]
pub async fn backup_create(state: State<'_, AppState>, path: String, include_files: bool, password: Option<String>) -> R<backup::BackupReport> {
    let lib = state.lib()?;
    let p = PathBuf::from(path);
    if !p.is_absolute() {
        return Err(AppError::Invalid("저장 위치가 올바르지 않습니다".into()));
    }
    let pw = password.filter(|s| !s.is_empty());
    blocking(move || backup::create(&*lib.conn()?, &lib.paths, &p, include_files, pw.as_deref())).await
}

#[tauri::command]
pub async fn backup_inspect(state: State<'_, AppState>, path: String, password: Option<String>) -> R<backup::RestorePlan> {
    let lib = state.lib()?;
    let pw = password.filter(|s| !s.is_empty());
    blocking(move || backup::plan_restore(&*lib.conn()?, Path::new(&path), pw.as_deref())).await
}

/// 복원: 현재 DB 연결을 닫고 → (안전 사본 생성 후) 교체 → 다시 연다
#[tauri::command]
pub async fn backup_restore(state: State<'_, AppState>, path: String, password: Option<String>, confirm_replace: bool) -> R<backup::RestoreReport> {
    let lib = state.lib()?;
    if !state.jobs.lock().unwrap().is_empty() {
        return Err(AppError::Invalid("진행 중인 작업이 끝난 뒤 복원해 주세요".into()));
    }
    let pw = password.filter(|s| !s.is_empty());
    let plan = { backup::plan_restore(&*lib.conn()?, Path::new(&path), pw.as_deref())? };
    if plan.conflict && !confirm_replace {
        return Err(AppError::Invalid("현재 연구실에 자료가 있습니다. 교체에 동의해야 복원할 수 있습니다.".into()));
    }
    let root = lib.paths.root.clone();
    let demo = lib.paths.is_demo();
    let report = {
        let mut guard = lib.conn()?;
        // 파일 핸들을 닫기 위해 메모리 DB로 바꿔 둔다
        let old = std::mem::replace(&mut *guard, prh_core::rusqlite::Connection::open_in_memory()?);
        drop(old);
        let r = backup::restore(&lib.paths, Path::new(&path), pw.as_deref());
        // 성공·실패와 관계없이 연구실 DB를 다시 연다
        match db::open(&lib.paths) {
            Ok((c, _)) => *guard = c,
            Err(e) => return Err(AppError::Backup(format!("복원 후 연구실을 다시 열지 못했습니다: {e}"))),
        }
        r?
    };
    *lib.index.write().unwrap() = None;
    let _ = state.switch_to(&root, demo);
    Ok(report)
}
