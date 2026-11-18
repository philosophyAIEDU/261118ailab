# RELEASE_CHECKLIST

## A. 최초 1회 준비 (관리자 승인 필요)
- [ ] **업데이트 서명 키 생성** (안전한 개인 컴퓨터에서): `npx tauri signer generate -w ~/.tauri/prh-updater.key`
  - 개인키·암호는 비밀번호 관리자에 보관. **저장소에 절대 커밋하지 않는다.** 분실 시 기존 설치본은 새 업데이트를 받을 수 없다.
- [ ] GitHub 저장소 → Settings → Secrets and variables → Actions
  - Secrets: `TAURI_SIGNING_PRIVATE_KEY`(개인키 파일 내용), `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
  - Variables: `TAURI_UPDATER_PUBKEY`(공개키 `.pub` 내용)
- [ ] (권장) macOS 서명·공증: Apple Developer ID 인증서 → Secrets `APPLE_CERTIFICATE`(base64 .p12), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`(앱 암호), `APPLE_TEAM_ID`
  - 서명·공증하지 않으면 Gatekeeper가 “확인되지 않은 개발자” 경고를 표시한다.
- [ ] (권장) Windows 코드 서명: OV/EV 인증서 또는 Azure Trusted Signing 검토 → `bundle.windows` 서명 설정 추가
  - 서명하지 않으면 SmartScreen 경고가 표시된다.

## B. 버전마다
- [ ] `package.json`, `Cargo.toml`(workspace.package.version), `src-tauri/tauri.conf.json`의 버전을 같은 값으로 올린다.
- [ ] 자료 형식이 바뀌면 `MIGRATIONS`에 **새 항목만 추가**했는지, 마이그레이션 테스트를 추가했는지 확인.
- [ ] `docs/release-notes/vX.Y.Z.md` 작성 — `### 개선된 기능`, `### 수정된 오류` 제목을 쓰면 앱 업데이트 창에 나뉘어 표시된다.
- [ ] `CHANGELOG.md` 갱신.
- [ ] 로컬 검증: `npm run build && npm test && cargo test -p prh-core && cargo test -p philosophy-research-hub`
- [ ] (가능하면) E2E: `scripts/e2e.cjs` (Linux, Xvfb)
- [ ] 태그: 안정 `vX.Y.Z`, 시험판 `vX.Y.Z-beta.N` → `git push origin <태그>` → `release` 워크플로가 서명된 설치 파일·`latest.json`을 올리고 게시, 베타 채널(`updater-beta`) 갱신.

## C. 게시 후 확인 (실기기)
- [ ] Windows: 설치 → 시작 메뉴 “교수님만의 AI 연구실” → 실행 → 이전 버전에서 앱 내부 업데이트 → 재실행 후 연구실 자료 유지
- [ ] macOS: DMG 설치 → 실행(서명·공증 상태 확인) → 앱 내부 업데이트 → 자료 유지
- [ ] 제거 후 재설치 → `문서/PhilosophyResearchHub` 그대로인지 확인
- [ ] 업데이트 서명 검증 실패 시 설치 거부되는지(잘못된 서명의 시험 릴리스로) 확인
- [ ] 실제 OS에서 확인한 결과만 TEST_RESULTS.md에 “검증 완료”로 기록 (확인하지 않은 항목은 “검증 필요”)
