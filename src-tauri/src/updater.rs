//! 앱 내부 업데이트 (Tauri 2 공식 Updater 플러그인).
//!
//! 흐름: 확인 → 새 버전·변경 사항·파일 크기 표시 → 사용자 승인 → 연구실 DB 안전 사본 생성 →
//! 다운로드 → **전자서명 검증(플러그인이 공개키로 수행, 실패 시 설치 거부)** → 설치 → 재실행.
//!
//! 업데이트 서명 공개키(tauri.conf.json `plugins.updater.pubkey`)가 비어 있으면
//! "아직 업데이트 서명이 설정되지 않음"으로 안내하고 확인·설치를 하지 않는다.
use crate::commands::{emit, JobEvent};
use crate::state::AppState;
use prh_core::{AppError, AppResult};
use serde_json::{json, Value};
use tauri::{AppHandle, State};
use tauri_plugin_updater::UpdaterExt;

const REPO: &str = "https://github.com/philosophyAIEDU/261118ailab";

pub fn endpoint(channel: &str) -> String {
    match channel {
        // 시험판(베타) 채널: 고정 태그 `updater-beta`의 latest.json (사전 출시 빌드가 갱신)
        "beta" => format!("{REPO}/releases/download/updater-beta/latest.json"),
        // 안정 채널: GitHub의 "최신 릴리스"(사전 출시 제외)
        _ => format!("{REPO}/releases/latest/download/latest.json"),
    }
}

fn pubkey(app: &AppHandle) -> String {
    app.config().plugins.0.get("updater").and_then(|u| u.get("pubkey")).and_then(|v| v.as_str()).unwrap_or("").trim().to_string()
}

/// 릴리스 노트에서 "개선된 기능"/"수정된 오류" 절을 나눈다
pub fn split_notes(body: &str) -> (Vec<String>, Vec<String>, String) {
    let mut feat = vec![];
    let mut fix = vec![];
    let mut cur: Option<&mut Vec<String>> = None;
    for line in body.lines() {
        let l = line.trim();
        let low = l.to_lowercase();
        if l.starts_with('#') {
            cur = if l.contains("개선") || l.contains("새 기능") || low.contains("feature") || low.contains("added") {
                Some(&mut feat)
            } else if l.contains("수정") || l.contains("오류") || low.contains("fix") {
                Some(&mut fix)
            } else {
                None
            };
            continue;
        }
        if let Some(v) = cur.as_deref_mut() {
            let item = l.trim_start_matches(['-', '*', '•']).trim();
            if !item.is_empty() {
                v.push(item.to_string());
            }
        }
    }
    (feat, fix, body.to_string())
}

#[tauri::command]
pub async fn update_check(app: AppHandle, state: State<'_, AppState>, channel: Option<String>) -> AppResult<Value> {
    let current = app.package_info().version.to_string();
    let channel = channel.unwrap_or_else(|| "stable".into());
    if pubkey(&app).is_empty() {
        return Ok(json!({"configured": false, "available": false, "current": current, "channel": channel,
            "message": "이 빌드에는 업데이트 서명 공개키가 아직 설정되지 않아 앱 내부 업데이트를 사용할 수 없습니다. 새 버전은 GitHub Releases에서 직접 내려받아 주세요."}));
    }
    let url = url::Url::parse(&endpoint(&channel)).map_err(|_| AppError::Invalid("업데이트 주소 오류".into()))?;
    let updater = app
        .updater_builder()
        .endpoints(vec![url])
        .map_err(|e| AppError::Invalid(format!("업데이트 설정 오류: {e}")))?
        .build()
        .map_err(|e| AppError::Invalid(format!("업데이트 설정 오류: {e}")))?;
    let upd = updater.check().await.map_err(|e| AppError::File(format!("업데이트 서버를 확인하지 못했습니다. 인터넷 연결을 확인해 주세요. ({e})")))?;
    let Some(u) = upd else {
        *state.pending_update.lock().await = None;
        return Ok(json!({"configured": true, "available": false, "current": current, "channel": channel}));
    };
    // 파일 크기 (서버가 알려 주는 경우에만)
    let size = reqwest::Client::new().head(u.download_url.clone()).send().await.ok().and_then(|r| r.content_length());
    let (features, fixes, body) = split_notes(u.body.as_deref().unwrap_or(""));
    let out = json!({
        "configured": true, "available": true, "current": current, "latest": u.version, "channel": channel,
        "date": u.date.map(|d| d.to_string()), "features": features, "fixes": fixes, "notes": body, "size_bytes": size,
    });
    *state.pending_update.lock().await = Some(u);
    Ok(out)
}

#[tauri::command]
pub async fn update_install(app: AppHandle, state: State<'_, AppState>, job: String) -> AppResult<()> {
    let u = state.pending_update.lock().await.take().ok_or_else(|| AppError::Invalid("먼저 업데이트를 확인해 주세요".into()))?;
    // 연구자료 보호: 설치 전에 연구실 DB 안전 사본을 만든다
    if let Ok(lib) = state.lib() {
        let c = lib.conn()?;
        prh_core::db::snapshot(&c, &lib.paths.backups(), &format!("pre-update-{}", u.version))?;
    }
    let app2 = app.clone();
    let j = job.clone();
    let mut got: usize = 0;
    u.download_and_install(
        move |chunk, total| {
            got += chunk;
            emit(&app2, JobEvent { id: j.clone(), kind: "update".into(), stage: "progress".into(), done: got, total: total.unwrap_or(0) as usize, message: "업데이트 내려받는 중".into(), data: Value::Null });
        },
        || {},
    )
    .await
    .map_err(|e| AppError::Blocked(format!("업데이트를 설치하지 못했습니다 (서명 검증 실패 또는 다운로드 오류). 기존 버전과 연구자료는 그대로입니다. ({e})")))?;
    emit(&app, JobEvent { id: job, kind: "update".into(), stage: "done".into(), done: 1, total: 1, message: "설치 완료".into(), data: Value::Null });
    Ok(())
}

#[tauri::command]
pub async fn app_restart(app: AppHandle) -> AppResult<()> {
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notes_split() {
        let (f, x, _) = split_notes("## 개선된 기능\n- 논문 비교 속도 향상\n- 그래프 필터\n\n## 수정된 오류\n* PDF 쪽 이동 오류");
        assert_eq!(f, vec!["논문 비교 속도 향상", "그래프 필터"]);
        assert_eq!(x, vec!["PDF 쪽 이동 오류"]);
        assert!(endpoint("beta").contains("updater-beta"));
        assert!(endpoint("stable").contains("/latest/download/"));
    }
}
