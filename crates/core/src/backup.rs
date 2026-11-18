//! 연구실 백업·검증·복원.
//!
//! 백업 파일(`.prhbak`)은 ZIP 형식이며 다음을 담는다.
//! - `manifest.json` : 형식 버전, 생성 시각, 자료 형식(스키마) 버전, 항목 수, 파일별 SHA-256
//! - `research.db`   : 논문 메타데이터·노트·프로젝트·비교·설정·검색 색인 전체 (일관된 스냅숏)
//! - `papers/…`      : (선택) PDF 등 원본 사본
//! - `readable/…`    : 사람이 읽을 수 있는 JSON·Markdown 사본 (프로그램 없이도 내용 확인 가능)
//!
//! 암호를 지정하면 ZIP 전체를 Argon2id로 만든 키와 XChaCha20-Poly1305로 1MB 단위 암호화한다
//! (`.prhbak.enc`). 암호를 잃어버리면 복구할 수 없다.
use crate::error::{AppError, AppResult};
use crate::paths::LibraryPaths;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 8] = b"PRHBAK\x01E";
const CHUNK: usize = 1 << 20;
pub const FORMAT: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFile {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub app: String,
    pub format: u32,
    pub created_at: String,
    pub schema_version: i64,
    pub include_files: bool,
    pub counts: serde_json::Value,
    pub files: Vec<ManifestFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupReport {
    pub path: String,
    pub size: u64,
    pub encrypted: bool,
    pub manifest: Manifest,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyReport {
    pub ok: bool,
    pub encrypted: bool,
    pub manifest: Option<Manifest>,
    pub problems: Vec<String>,
}

fn sha_bytes(b: &[u8]) -> String {
    hex::encode(Sha256::digest(b))
}

fn derive_key(password: &str, salt: &[u8]) -> AppResult<[u8; 32]> {
    // 테스트에서는 속도를 위해 비용을 낮춘다 (배포 빌드: 64MB, 3회)
    let (m, t) = if cfg!(test) { (8 * 1024, 1) } else { (64 * 1024, 3) };
    let params = argon2::Params::new(m, t, 1, Some(32)).map_err(|_| AppError::Backup("암호화 설정 오류".into()))?;
    let a = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut key = [0u8; 32];
    a.hash_password_into(password.as_bytes(), salt, &mut key).map_err(|_| AppError::Backup("암호 키를 만들지 못했습니다".into()))?;
    Ok(key)
}

fn nonce_for(base: &[u8; 16], counter: u64) -> XNonce {
    let mut n = [0u8; 24];
    n[..16].copy_from_slice(base);
    n[16..].copy_from_slice(&counter.to_be_bytes());
    *XNonce::from_slice(&n)
}

/// 파일을 청크 단위로 암호화한다. 마지막 청크 여부를 인증 데이터(AAD)에 넣어 잘라내기 공격을 막는다.
pub fn encrypt_file(src: &Path, dst: &Path, password: &str) -> AppResult<()> {
    if password.chars().count() < 8 {
        return Err(AppError::Invalid("백업 암호는 8자 이상이어야 합니다".into()));
    }
    let mut salt = [0u8; 16];
    let mut base = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    rand::thread_rng().fill_bytes(&mut base);
    let key = derive_key(password, &salt)?;
    let cipher = XChaCha20Poly1305::new((&key).into());
    let mut input = std::fs::File::open(src)?;
    let total = input.metadata()?.len();
    let mut out = std::io::BufWriter::new(std::fs::File::create(dst)?);
    out.write_all(MAGIC)?;
    out.write_all(&salt)?;
    out.write_all(&base)?;
    let mut buf = vec![0u8; CHUNK];
    let mut read_total = 0u64;
    let mut counter = 0u64;
    loop {
        let mut n = 0;
        while n < CHUNK {
            let r = input.read(&mut buf[n..])?;
            if r == 0 {
                break;
            }
            n += r;
        }
        read_total += n as u64;
        let last = read_total >= total;
        let aad = [counter.to_be_bytes().as_slice(), &[last as u8]].concat();
        let ct = cipher.encrypt(&nonce_for(&base, counter), Payload { msg: &buf[..n], aad: &aad }).map_err(|_| AppError::Backup("암호화 실패".into()))?;
        out.write_all(&[last as u8])?;
        out.write_all(&(ct.len() as u32).to_be_bytes())?;
        out.write_all(&ct)?;
        counter += 1;
        if last {
            break;
        }
    }
    out.flush()?;
    Ok(())
}

pub fn is_encrypted(path: &Path) -> AppResult<bool> {
    let mut f = std::fs::File::open(path)?;
    let mut m = [0u8; 8];
    Ok(f.read_exact(&mut m).is_ok() && &m == MAGIC)
}

pub fn decrypt_file(src: &Path, dst: &Path, password: &str) -> AppResult<()> {
    let mut f = std::io::BufReader::new(std::fs::File::open(src)?);
    let mut magic = [0u8; 8];
    let mut salt = [0u8; 16];
    let mut base = [0u8; 16];
    f.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(AppError::Backup("암호화된 백업 파일이 아닙니다".into()));
    }
    f.read_exact(&mut salt)?;
    f.read_exact(&mut base)?;
    let key = derive_key(password, &salt)?;
    let cipher = XChaCha20Poly1305::new((&key).into());
    let mut out = std::io::BufWriter::new(std::fs::File::create(dst)?);
    let mut counter = 0u64;
    loop {
        let mut hdr = [0u8; 5];
        if f.read_exact(&mut hdr).is_err() {
            return Err(AppError::Backup("백업 파일이 잘렸거나 손상되었습니다".into()));
        }
        let last = hdr[0] == 1;
        let len = u32::from_be_bytes([hdr[1], hdr[2], hdr[3], hdr[4]]) as usize;
        if len > CHUNK + 64 {
            return Err(AppError::Backup("백업 파일 형식이 올바르지 않습니다".into()));
        }
        let mut ct = vec![0u8; len];
        f.read_exact(&mut ct).map_err(|_| AppError::Backup("백업 파일이 잘렸거나 손상되었습니다".into()))?;
        let aad = [counter.to_be_bytes().as_slice(), &[last as u8]].concat();
        let pt = cipher
            .decrypt(&nonce_for(&base, counter), Payload { msg: &ct, aad: &aad })
            .map_err(|_| AppError::Backup("암호가 틀렸거나 백업 파일이 손상되었습니다".into()))?;
        out.write_all(&pt)?;
        counter += 1;
        if last {
            break;
        }
    }
    out.flush()?;
    Ok(())
}

fn counts(conn: &Connection) -> AppResult<serde_json::Value> {
    let q = |s: &str| -> AppResult<i64> { Ok(conn.query_row(s, [], |r| r.get(0))?) };
    Ok(serde_json::json!({
        "papers": q("SELECT count(*) FROM papers")?,
        "notes": q("SELECT count(*) FROM notes")?,
        "projects": q("SELECT count(*) FROM projects")?,
        "comparisons": q("SELECT count(*) FROM comparisons")?,
        "reviews": q("SELECT count(*) FROM reviews")?,
        "annotations": q("SELECT count(*) FROM annotations")?,
    }))
}

fn readable_exports(conn: &Connection) -> AppResult<Vec<(String, Vec<u8>)>> {
    let papers = crate::library::list_papers(conn, &Default::default())?;
    let notes = crate::notes::list(conn, &Default::default())?;
    let projects = crate::projects::list(conn)?;
    let settings = crate::settings::load(conn)?;
    let mut v = vec![
        ("readable/papers.json".to_string(), serde_json::to_vec_pretty(&papers)?),
        ("readable/notes.json".into(), serde_json::to_vec_pretty(&notes)?),
        ("readable/projects.json".into(), serde_json::to_vec_pretty(&projects)?),
        ("readable/settings.json".into(), serde_json::to_vec_pretty(&settings)?),
    ];
    for n in &notes {
        let stem = crate::export::safe_file_stem(&n.title);
        v.push((format!("readable/notes/{stem}-{}.md", &n.id[..8]), format!("# {}\n\n{}\n", n.title, n.body).into_bytes()));
    }
    Ok(v)
}

/// 백업 만들기. `dest`는 사용자가 고른 저장 경로.
pub fn create(conn: &Connection, paths: &LibraryPaths, dest: &Path, include_files: bool, password: Option<&str>) -> AppResult<BackupReport> {
    if dest.starts_with(paths.papers()) {
        return Err(AppError::Invalid("백업 파일을 연구실의 papers 폴더 안에 저장할 수 없습니다".into()));
    }
    if let Some(p) = password {
        if p.chars().count() < 8 {
            return Err(AppError::Invalid("백업 암호는 8자 이상이어야 합니다".into()));
        }
    }
    let work = tempdir_in(&paths.backups())?;
    let snap = crate::db::snapshot(conn, &work, "backup")?;
    let schema: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let mut files = Vec::new();
    let zip_path = work.join("backup.zip");
    {
        let mut zw = zip::ZipWriter::new(std::fs::File::create(&zip_path)?);
        let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).large_file(true);
        let mut add = |zw: &mut zip::ZipWriter<std::fs::File>, name: &str, src: &Path, o: zip::write::SimpleFileOptions| -> AppResult<()> {
            let mut f = std::fs::File::open(src)?;
            let mut h = Sha256::new();
            let mut size = 0u64;
            zw.start_file(name, o)?;
            let mut buf = vec![0u8; 1 << 16];
            loop {
                let n = f.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
                zw.write_all(&buf[..n])?;
                size += n as u64;
            }
            files.push(ManifestFile { path: name.into(), sha256: hex::encode(h.finalize()), size });
            Ok(())
        };
        add(&mut zw, "research.db", &snap, opt)?;
        if paths.marker().exists() {
            add(&mut zw, "library.json", &paths.marker(), opt)?;
        }
        if include_files {
            let mut st = conn.prepare("SELECT file_path FROM papers WHERE file_path IS NOT NULL")?;
            let rels: Vec<String> = st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
            for rel in rels {
                let abs = paths.resolve_stored(&rel)?;
                if abs.is_file() {
                    add(&mut zw, &rel, &abs, stored)?;
                }
            }
        }
        for (name, bytes) in readable_exports(conn)? {
            zw.start_file(&name, opt)?;
            zw.write_all(&bytes)?;
            files.push(ManifestFile { path: name, sha256: sha_bytes(&bytes), size: bytes.len() as u64 });
        }
        let manifest = Manifest { app: "philosophy-research-hub".into(), format: FORMAT, created_at: crate::now(), schema_version: schema, include_files, counts: counts(conn)?, files: files.clone() };
        zw.start_file("manifest.json", opt)?;
        zw.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
        zw.finish()?;
    }
    let tmp_dest = dest.with_extension("partial");
    match password {
        Some(p) => encrypt_file(&zip_path, &tmp_dest, p)?,
        None => {
            std::fs::copy(&zip_path, &tmp_dest)?;
        }
    }
    std::fs::rename(&tmp_dest, dest)?;
    let _ = std::fs::remove_dir_all(&work);
    let v = verify(dest, password)?;
    if !v.ok {
        return Err(AppError::Backup(format!("백업 검증에 실패했습니다: {}", v.problems.join(", "))));
    }
    Ok(BackupReport { path: dest.to_string_lossy().into_owned(), size: std::fs::metadata(dest)?.len(), encrypted: password.is_some(), manifest: v.manifest.unwrap() })
}

fn tempdir_in(dir: &Path) -> AppResult<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let p = dir.join(format!(".work-{}", &crate::new_id()[..8]));
    std::fs::create_dir_all(&p)?;
    Ok(p)
}

/// 백업을 열어 볼 수 있는 ZIP 경로로 만든다 (암호화된 경우 임시 복호화)
fn open_plain(path: &Path, password: Option<&str>, work: &Path) -> AppResult<(PathBuf, bool)> {
    if is_encrypted(path)? {
        let pw = password.ok_or_else(|| AppError::Invalid("암호화된 백업입니다. 암호를 입력해 주세요.".into()))?;
        let out = work.join("decrypted.zip");
        decrypt_file(path, &out, pw)?;
        Ok((out, true))
    } else {
        Ok((path.to_path_buf(), false))
    }
}

/// 백업 파일 검증: 파일별 해시, DB 무결성, 자료 형식 버전
pub fn verify(path: &Path, password: Option<&str>) -> AppResult<VerifyReport> {
    let work = std::env::temp_dir().join(format!("prh-verify-{}", &crate::new_id()[..8]));
    std::fs::create_dir_all(&work)?;
    let r = verify_inner(path, password, &work);
    let _ = std::fs::remove_dir_all(&work);
    r
}

fn verify_inner(path: &Path, password: Option<&str>, work: &Path) -> AppResult<VerifyReport> {
    let (plain, encrypted) = match open_plain(path, password, work) {
        Ok(x) => x,
        Err(e) => return Ok(VerifyReport { ok: false, encrypted: is_encrypted(path).unwrap_or(false), manifest: None, problems: vec![e.to_string()] }),
    };
    let mut z = match zip::ZipArchive::new(std::fs::File::open(&plain)?) {
        Ok(z) => z,
        Err(_) => return Ok(VerifyReport { ok: false, encrypted, manifest: None, problems: vec!["백업 파일 형식이 아닙니다".into()] }),
    };
    let manifest: Manifest = {
        let Ok(mut m) = z.by_name("manifest.json") else {
            return Ok(VerifyReport { ok: false, encrypted, manifest: None, problems: vec!["백업 정보(manifest.json)가 없습니다".into()] });
        };
        let mut s = String::new();
        m.read_to_string(&mut s)?;
        serde_json::from_str(&s)?
    };
    let mut problems = vec![];
    if manifest.app != "philosophy-research-hub" {
        problems.push("이 프로그램의 백업 파일이 아닙니다".into());
    }
    if manifest.format > FORMAT {
        problems.push("더 새로운 버전의 프로그램에서 만든 백업입니다. 프로그램을 업데이트해 주세요.".into());
    }
    if manifest.schema_version > crate::db::latest_version() {
        problems.push(format!("백업의 자료 형식(v{})이 이 프로그램(v{})보다 새롭습니다. 프로그램을 업데이트해 주세요.", manifest.schema_version, crate::db::latest_version()));
    }
    for f in &manifest.files {
        let Ok(mut e) = z.by_name(&f.path) else {
            problems.push(format!("누락된 파일: {}", f.path));
            continue;
        };
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 1 << 16];
        let mut broken = false;
        loop {
            match e.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => h.update(&buf[..n]),
                Err(_) => {
                    broken = true;
                    break;
                }
            }
        }
        if broken || hex::encode(h.finalize()) != f.sha256 {
            problems.push(format!("손상된 파일: {}", f.path));
        }
    }
    // DB 무결성
    if problems.is_empty() {
        let dbp = work.join("check.db");
        {
            let mut e = z.by_name("research.db")?;
            let mut out = std::fs::File::create(&dbp)?;
            if std::io::copy(&mut e, &mut out).is_err() {
                problems.push("백업 안의 데이터베이스를 읽지 못했습니다".into());
            }
        }
        match Connection::open(&dbp).map_err(AppError::from).and_then(|c| crate::db::integrity_ok(&c)) {
            Ok(true) => {}
            _ => problems.push("백업 안의 데이터베이스가 손상되었습니다".into()),
        }
    }
    Ok(VerifyReport { ok: problems.is_empty(), encrypted, manifest: Some(manifest), problems })
}

#[derive(Debug, Clone, Serialize)]
pub struct RestorePlan {
    pub backup: Manifest,
    pub encrypted: bool,
    pub current_counts: serde_json::Value,
    /// 현재 연구실에 자료가 있어 복원 시 교체됨
    pub conflict: bool,
}

/// 복원 전 확인: 백업 검증 + 현재 연구실과의 충돌 여부
pub fn plan_restore(conn: &Connection, path: &Path, password: Option<&str>) -> AppResult<RestorePlan> {
    let v = verify(path, password)?;
    if !v.ok {
        return Err(AppError::Backup(format!("복원할 수 없는 백업입니다: {}", v.problems.join(", "))));
    }
    let cur = counts(conn)?;
    let conflict = ["papers", "notes", "projects"].iter().any(|k| cur[k].as_i64().unwrap_or(0) > 0);
    Ok(RestorePlan { backup: v.manifest.unwrap(), encrypted: v.encrypted, current_counts: cur, conflict })
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreReport {
    pub safety_backup: Option<String>,
    pub files_restored: usize,
}

/// 복원 실행. **호출 전에 연구실 DB 연결을 닫아야 한다.**
/// 현재 DB는 먼저 `backups/`에 안전 사본으로 보관한 뒤 교체한다. PDF 사본은 백업에 있는 것만 추가한다
/// (기존 파일은 지우지 않음).
pub fn restore(paths: &LibraryPaths, path: &Path, password: Option<&str>) -> AppResult<RestoreReport> {
    let v = verify(path, password)?;
    if !v.ok {
        return Err(AppError::Backup(format!("복원할 수 없는 백업입니다: {}", v.problems.join(", "))));
    }
    paths.ensure(false)?;
    let work = tempdir_in(&paths.backups())?;
    let result = (|| -> AppResult<RestoreReport> {
        let (plain, _) = open_plain(path, password, &work)?;
        let mut z = zip::ZipArchive::new(std::fs::File::open(&plain)?)?;
        // 1) 현재 DB 안전 사본
        let mut safety = None;
        if paths.db().exists() {
            let cur = Connection::open(paths.db())?;
            safety = Some(crate::db::snapshot(&cur, &paths.backups(), "pre-restore")?.to_string_lossy().into_owned());
        }
        // 2) 새 DB를 임시로 풀고 무결성 확인 후 교체
        let new_db = work.join("research.db");
        {
            let mut e = z.by_name("research.db")?;
            std::io::copy(&mut e, &mut std::fs::File::create(&new_db)?)?;
        }
        if !crate::db::integrity_ok(&Connection::open(&new_db)?)? {
            return Err(AppError::Backup("백업 데이터베이스가 손상되었습니다".into()));
        }
        for ext in ["-wal", "-shm"] {
            let p = PathBuf::from(format!("{}{}", paths.db().to_string_lossy(), ext));
            let _ = std::fs::remove_file(p);
        }
        std::fs::copy(&new_db, paths.db())?;
        // 3) 첨부 파일
        let mut n = 0;
        for i in 0..z.len() {
            let mut e = z.by_index(i)?;
            let name = e.name().to_string();
            if !name.starts_with("papers/") || e.is_dir() {
                continue;
            }
            let dest = paths.resolve_stored(&name)?; // 경로 검증 (zip slip 방지)
            if dest.exists() {
                continue;
            }
            let tmp = dest.with_extension("part");
            std::io::copy(&mut e, &mut std::fs::File::create(&tmp)?)?;
            std::fs::rename(&tmp, &dest)?;
            n += 1;
        }
        Ok(RestoreReport { safety_backup: safety, files_restored: n })
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::{lib, write_pdf};

    #[test]
    fn backup_verify_restore_roundtrip_plain_and_encrypted() {
        let (d, paths, mut c) = lib();
        let a = write_pdf(d.path(), "a.pdf", &["Backup test paper with enough words to index properly"], Some("Backup Paper"));
        crate::library::import_file(&mut c, &paths, &a).unwrap();
        crate::notes::create(&mut c, &crate::notes::NoteInput { title: "중요 노트".into(), body: "내용".into(), project_id: None, tags: vec![], paper_ids: vec![], from_ai_draft: false }).unwrap();
        for pw in [None, Some("correct horse battery")] {
            let dest = d.path().join(if pw.is_some() { "b.prhbak.enc" } else { "b.prhbak" });
            let rep = create(&c, &paths, &dest, true, pw).unwrap();
            assert_eq!(rep.encrypted, pw.is_some());
            assert!(rep.manifest.files.iter().any(|f| f.path.starts_with("papers/")));
            assert!(verify(&dest, pw).unwrap().ok);
            if pw.is_some() {
                let bad = verify(&dest, Some("wrong password!")).unwrap();
                assert!(!bad.ok);
                assert!(bad.problems[0].contains("암호"));
            }
            // 새 연구실로 복원
            let target = LibraryPaths::new(d.path().join(format!("restored-{}", pw.is_some())));
            target.ensure(false).unwrap();
            let r = restore(&target, &dest, pw).unwrap();
            assert_eq!(r.files_restored, 1);
            let (rc, _) = crate::db::open(&target).unwrap();
            assert_eq!(crate::library::stats(&rc).unwrap().papers, 1);
            assert_eq!(crate::library::stats(&rc).unwrap().notes, 1);
            let p = &crate::library::list_papers(&rc, &Default::default()).unwrap()[0];
            assert!(crate::library::paper_file_abs(&rc, &target, &p.id).is_ok());
        }
    }

    #[test]
    fn tampered_backup_is_detected_and_conflict_reported() {
        let (d, paths, c) = lib();
        let dest = d.path().join("x.prhbak");
        create(&c, &paths, &dest, false, None).unwrap();
        let plan = plan_restore(&c, &dest, None).unwrap();
        assert!(!plan.conflict);
        let mut bytes = std::fs::read(&dest).unwrap();
        // ZIP 중앙 디렉터리 이전 영역의 바이트를 훼손
        let mid = bytes.len() / 3;
        bytes[mid] ^= 0xFF;
        std::fs::write(&dest, &bytes).unwrap();
        let v = verify(&dest, None).unwrap();
        assert!(!v.ok);
    }

    #[test]
    fn encryption_detects_truncation() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("s");
        std::fs::write(&src, vec![7u8; CHUNK * 2 + 10]).unwrap();
        let enc = d.path().join("e");
        encrypt_file(&src, &enc, "password123").unwrap();
        let dec = d.path().join("d");
        decrypt_file(&enc, &dec, "password123").unwrap();
        assert_eq!(std::fs::read(&dec).unwrap(), std::fs::read(&src).unwrap());
        // 마지막 청크 잘라내기
        let b = std::fs::read(&enc).unwrap();
        let cut = &b[..b.len() - (10 + 16 + 5)];
        std::fs::write(&enc, cut).unwrap();
        assert!(decrypt_file(&enc, &dec, "password123").is_err());
        assert!(encrypt_file(&src, &enc, "short").is_err());
    }
}
