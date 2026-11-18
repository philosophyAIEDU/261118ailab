# ARCHITECTURE — Philosophy Research Hub

## 기술 선택
| 영역 | 선택 | 이유 |
|---|---|---|
| 앱 틀 | **Tauri 2** (Rust + 시스템 웹뷰) | 설치 파일이 작고(Windows/macOS), 공식 Updater·서명 검증, 같은 팀 Philosophy Local AI와 같은 스택 |
| 화면 | React 19 + TypeScript + Vite + Tailwind CSS 4 | 표준 생태계, 빠른 빌드 |
| 저장 | SQLite (rusqlite, 번들 빌드) + **FTS5** | 단일 파일, 트랜잭션, 전문 검색 내장 |
| 벡터 검색 | SQLite BLOB(f32) + 메모리 코사인 검색 | 논문 수백 편 규모에서 충분(120편·370문단 검색 1.5ms), 외부 의존성 없음 |
| PDF 본문 | `pdf-extract` + `lopdf` (Rust, 쪽 단위) | 쪽 번호 보존, 한글 CID 글꼴 지원 확인 |
| PDF 표시 | PDF.js 5 (`pdfjs-dist`), CMap·표준 글꼴 로컬 포함 | 오프라인 동작, 텍스트 레이어로 선택·하이라이트 |
| 로컬 AI | Ollama HTTP API (127.0.0.1) | 사용자가 모델을 고르고 내려받기 쉬움, 임베딩·생성 모두 지원 |
| 클라우드 AI (선택) | Gemini API REST(SSE 스트리밍) | 사양이 낮은 컴퓨터용 대안. `llm.rs`가 Ollama/Gemini를 같은 방식으로 감싸고, 사용량은 `usage.rs`(태평양 시간 일 단위)에 기록 |
| 그래프 | d3-force + SVG | 가볍고 제어하기 쉬움 |
| 문서 생성 | docx-rs, rust_xlsxwriter, csv | 실제 DOCX/XLSX 파일 생성 |
| 배포 | GitHub Actions + `tauri-action` + GitHub Releases | 서명된 업데이트 산출물(latest.json) 자동 생성 |

## 계층
```
┌────────────── React 화면 (src/) ──────────────┐
│ pages/*  components/*  lib/api.ts(invoke)     │
└───────────────▲──────────────────┬────────────┘
                │ 이벤트 "job"      │ invoke
┌───────────────┴──────────────────▼────────────┐
│ src-tauri: commands.rs · updater.rs · state.rs │  ← 연구실 열기/전환, 작업 취소, 경로 검증
└───────────────▲──────────────────┬────────────┘
                │                  │
┌───────────────┴──────────────────▼────────────┐
│ crates/core (prh-core) — Tauri 비의존, 테스트 가능 │
│ db · paths · extract · metadata · bib · library │
│ text · search · ollama · rag · compare · review │
│ notes · projects · annotations · graph          │
│ export · backup · settings · models · demo      │
└───────────────▲──────────────────┬────────────┘
        SQLite(연구실 폴더)      Ollama(127.0.0.1:11434)
```

- 핵심 로직은 `prh-core`에 두어 Tauri 없이 단위·통합 테스트한다 (가짜 Ollama 서버 포함).
- 긴 작업(등록·색인·비교·문헌고찰·모델 내려받기)은 job id로 진행률·토큰을 `job` 이벤트로 보내고 `job_cancel`로 중단한다.
- 본문 추출 같은 무거운 처리는 DB 잠금 없이 수행하고, 저장할 때만 잠근다.

## 데이터 위치 (업데이트 후 보존)
| 무엇 | 어디 | 비고 |
|---|---|---|
| 연구실 (DB·PDF 사본·자동 백업) | `문서/PhilosophyResearchHub` (사용자가 변경 가능) | 앱 설치 폴더·앱 데이터 폴더와 분리 → 제거·재설치 시에도 유지 |
| 데모 연구실 | `문서/PhilosophyResearchHub-Demo` | 실제 연구실과 분리 |
| 연구실 위치 설정 | 앱 설정 폴더 `location.json` (`kr.philosophy-ai.research-hub`) | 없어져도 기본 연구실을 다시 연다 |
| AI 모델 | Ollama가 관리 | 연구실 백업 대상 아님 |

이름(제품명)이 바뀌어도 식별자·폴더 이름은 그대로 두어 하위 호환을 유지한다.

## 스키마 마이그레이션
`PRAGMA user_version` 기반. 기존 데이터가 있으면 업그레이드 전에 `backups/research-pre-migration-vA-to-vB-*.db`로 온라인 백업한 뒤
하나의 트랜잭션에서 실행한다. 실패하면 롤백된다. 앱보다 새 스키마의 DB는 열지 않는다(구버전 앱의 손상 방지).
자세한 내용은 [DATA_MODEL.md](DATA_MODEL.md).

## 업데이트 구조
- 플러그인: `tauri-plugin-updater` (공식). 공개키는 `tauri.conf.json`의 `plugins.updater.pubkey` — 현재 빈 값이며 배포 워크플로가 `TAURI_UPDATER_PUBKEY` 변수로 주입한다.
- 공개키가 없는 빌드는 업데이트 확인을 하지 않고 “서명이 설정되지 않음”을 안내한다.
- 채널: 안정 = `releases/latest/download/latest.json`, 베타 = `releases/download/updater-beta/latest.json` (Rust에서 채널별 엔드포인트로 빌더 생성).
- 흐름: 확인 → 버전·변경 사항·파일 크기(HEAD) 표시 → 사용자 승인 → 연구실 DB 안전 사본(`pre-update-*.db`) → 다운로드 → 서명 검증 → 설치 → 재실행.

## 네트워크
| 대상 | 용도 | 연구자료 전송 |
|---|---|---|
| 127.0.0.1:11434 (Ollama) | 임베딩·답변 생성·모델 관리 | 로컬 프로세스에만 |
| ollama.com (Ollama가 접속) | 모델 내려받기 | 없음 |
| github.com | 업데이트 확인·내려받기 | 없음 (버전·OS 정보만) |
| generativelanguage.googleapis.com | **사용자가 Gemini를 선택·동의한 경우만** 답변 생성 | 질문과 검색된 발췌 문단 (PDF 전체·노트·임베딩 없음) |

Ollama 클라이언트는 루프백 주소만 허용하고 시스템 프록시를 쓰지 않으며, 클라우드 모델(`*-cloud`)은 차단한다.
