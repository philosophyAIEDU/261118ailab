# DATA_MODEL — SQLite 스키마 (자료 형식 v1)

파일: `<연구실>/research.db` · 정의: `crates/core/src/db.rs` `MIGRATIONS[0]`

| 표 | 내용 | 주요 열 |
|---|---|---|
| `papers` | 논문·자료 1건 | id(UUID), title, authors(JSON), year, journal, doi, abstract, keywords(JSON), field, memo, file_path(연구실 기준 상대 경로), original_name, file_hash(SHA-256, 고유), file_kind, file_size, page_count, text_status(`pending/ok/partial/no_text/failed/metadata_only`), status_detail, **meta_source**(필드별 출처 JSON), bib_key, bib_type |
| `paper_tags` | 태그 | (paper_id, tag) |
| `chunks` | 검색 단위 문단 | id, paper_id, **page**(PDF 실제 쪽, 1부터 / 쪽 없는 형식은 NULL), ord, text |
| `chunks_fts` | FTS5 색인 (rowid = chunks.id) | tokens (한글 bigram + 소문자 단어) |
| `papers_fts` | 서지정보 색인 | paper_id, tokens |
| `embeddings` | 임베딩 벡터 | chunk_id, model, dim, vec(f32 LE, 정규화) |
| `annotations` | 하이라이트·메모·북마크 | paper_id, page, kind, text, note, color, rects(쪽 대비 비율 JSON) |
| `projects` / `project_papers` | 연구 프로젝트 | name, description, status(`active/paused/done`) |
| `notes` / `note_papers` / `note_tags` / `notes_fts` | 연구 노트 | title, body(Markdown), project_id, origin(`user`/`ai_reviewed`) |
| `links` | 그래프 연결 | src/dst(type,id), kind(`user`/`ai_suggested`/`dismissed`), label, score |
| `citations` | **실제 인용 관계** | citing_id, cited_id, evidence(`doi`/`title`), page |
| `searches` | AI 검색 기록 | query, mode, answer, result(JSON: 근거·검증 상태), model |
| `comparisons` | 비교표 | paper_ids, aspects, result(JSON: 칸별 값·상태·근거 쪽) |
| `reviews` | 문헌고찰 초안 | topic, paper_ids, content(JSON) |
| `settings` | 연구실 설정 | key=`app_settings` → 모델·포트·업데이트 채널 등 |

## 출처(meta_source) 값
`pdf_info`(PDF 속성) · `text`(본문 표지에서 찾음) · `text_guess`(배치로 추정, 확인 필요) · `file_name` · `user` · `bibtex` · `ris` · `demo`(가상 데모)

사용자가 수정한 필드는 `user`가 되며, 서지 파일 가져오기·재처리는 `user` 값을 덮어쓰지 않는다.

## 파일 저장
- 원본은 `papers/<sha256 앞 20자>.<확장자>`로 **복사**해 쓴다. 원본은 수정하지 않는다.
- DB에는 상대 경로만 저장하며, 읽을 때 `..`·절대 경로를 거부한다(`paths::resolve_stored`).

## 마이그레이션 규칙
1. 배포된 마이그레이션 문자열은 수정하지 않는다. 변경은 `MIGRATIONS`에 새 항목을 추가한다.
2. 기존 DB(버전 ≥ 1)를 올리기 전에 `backups/`에 온라인 백업한다.
3. 전체를 한 트랜잭션으로 실행한다 (실패 시 롤백 — 테스트 `failed_migration_rolls_back`).
4. DB 버전 > 앱 지원 버전이면 열지 않는다 (`refuses_newer_schema`).

## 백업 파일 형식 (`.prhbak`, 형식 1)
ZIP: `manifest.json`(앱·형식·스키마 버전·항목 수·파일별 SHA-256) + `research.db`(온라인 백업 스냅숏) + `library.json` + (선택) `papers/*` + `readable/*`(JSON·Markdown 사본).
암호화(`.prhbak.enc`): `PRHBAK\x01E` + salt(16) + nonce 기반(16) + [last(1) | len(4) | XChaCha20-Poly1305 암호문]×n, 키 = Argon2id(64MiB, 3회). 청크 번호·마지막 여부를 AAD로 인증해 잘라내기·순서 변경을 감지한다.
