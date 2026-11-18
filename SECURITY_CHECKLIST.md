# SECURITY_CHECKLIST

| # | 항목 | 상태 | 근거 |
|---|---|---|---|
| 1 | 연구자료 외부 전송 기본 금지 | ✅ | Ollama 클라이언트는 루프백만 허용(`Ollama::with_host`, 테스트 `only_loopback_allowed`), 그 외 네트워크는 업데이트(GitHub)·모델 내려받기(Ollama)뿐 |
| 2 | 로컬 요청의 프록시 우회 | ✅ | `reqwest::Client::builder().no_proxy()` |
| 3 | 사용자 승인 없는 클라우드 AI 금지 | ✅ | 기본은 로컬. Gemini는 사용자가 선택 + 전송 내용·무료 티어 데이터 이용·비용 안내 동의(`gemini_consent_at`) + 키 저장 후에만 사용, 동의 없이 설정 저장 불가(테스트 `gemini_requires_consent_and_defaults_are_local`). 전송 범위는 질문과 발췌 문단만, 임베딩은 항상 로컬. Gemini 주소는 Google로 고정(시험용 루프백만 예외). Ollama 클라우드 모델(`*-cloud`) 차단 |
| 4 | API 키 안전 보관 | ✅ | Windows 자격 증명 관리자·macOS 키체인(keyring), Linux 개발용은 0600 파일. DB·백업·로그 미포함, 헤더로만 전송(URL에 넣지 않음), 화면에는 앞뒤 4자만 표시 |
| 5 | 파일 경로 검증 | ✅ | DB 저장 경로는 상대 경로만, `..`·절대 경로 거부(`paths::resolve_stored`), 복원 시 zip slip 방지, 내보내기 경로는 절대 경로·확장자 확인·연구실 내부 폴더 금지, 연구실 폴더는 빈 폴더 또는 기존 연구실만 |
| 6 | PDF 내 악성 지시문(프롬프트 주입) 방어 | ✅ | `<source>` 격리·태그 무력화·지침, 의심 문장 표시, 답변의 링크·이미지 미렌더링 (`RAG_DESIGN.md` §4) |
| 7 | 원본 파일 무결성 | ✅ | 원본은 읽기만, 연구실에 사본 저장, 삭제 시 사본만 삭제(테스트 `import_dedupe_search_delete_keeps_original`) |
| 8 | 손상·악성 파일 처리 | ✅ | PDF 파서 패닉 격리(`catch_unwind`), 파일 300MB·XML 60MB 제한, quick-xml(외부 엔티티 미해석) |
| 9 | 오류 로그에 연구자료 원문 최소화 | ✅ | 오류 메시지는 한국어 요약만(경로·본문 미포함), 별도 로그 파일 없음 |
| 10 | 업데이트 파일 서명 검증 | ✅(구현) / ⏳(실배포 검증 필요) | 공식 Updater 플러그인이 공개키로 검증. 공개키가 없는 빌드는 업데이트를 수행하지 않음 |
| 11 | 서명 개인키 저장소 미포함 | ✅ | `.gitignore`(`*.key`), GitHub Actions Secrets 사용 |
| 12 | 개인정보 최소 수집 | ✅ | 계정·원격 측정 없음. 업데이트 확인 시 버전·OS 정보만 |
| 13 | 데이터 삭제·백업 기능 | ✅ | 논문·노트·프로젝트·비교·검색 기록 삭제, 백업·검증·복원 |
| 14 | 백업 암호화 | ✅ | Argon2id + XChaCha20-Poly1305, 청크 인증(잘라내기 감지 테스트) |
| 15 | 업데이트·마이그레이션 중 데이터 보호 | ✅ | 설치 전 DB 사본, 마이그레이션 전 백업·트랜잭션·롤백, 새 스키마 DB 열기 거부 |
| 16 | 웹뷰 보안 | ✅ | CSP(`default-src 'self'`, `script-src 'self'`, `connect-src ipc:`), `freezePrototype`, 최소 권한 capability(대화상자·이벤트), 외부 URL은 허용 목록만 |
| 17 | 단일 인스턴스 | ✅ | 같은 DB를 두 프로세스가 동시에 쓰지 않도록 `single-instance` |
| 18 | 코드 서명·공증 | ⏳ | Windows 코드 서명·macOS 공증은 인증서 준비 후 (RELEASE_CHECKLIST) |

> 로컬 AI를 사용한다고 해서 완전한 보안이 자동으로 보장되지는 않는다. 디스크 암호화, OS 업데이트, 백업 파일 보관 관리가 함께 필요하다는 점을 앱의 “보안·연구윤리” 화면에서 안내한다.
