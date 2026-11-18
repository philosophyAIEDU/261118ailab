# NEXT_TASK

## 우선순위 1 — 실제 환경 검증 (사용자 환경·승인 필요)
1. **실제 Ollama 모델로 검증**: qwen3:8b(답변), bge-m3(임베딩)로 데모 연구실에서 AI 검색·비교·문헌고찰 품질 확인.
   - 비교표 JSON 응답 형식 준수율, 한국어 답변 품질, `[S#]` 표기 준수율 측정 → 필요 시 프롬프트 조정.
   - 메모리 8GB 컴퓨터에서 gemma3:1b/4b 동작과 “메모리 부족” 안내 확인.
2. **Windows·macOS 실기기 시험**: GitHub Actions `build` 워크플로 실행 → 설치 파일로 설치·실행·제거·재설치 후 연구실 유지 확인.
3. **업데이트 서명 설정** (교수님/관리자 승인 후): `npx tauri signer generate` → 개인키를 Secrets(`TAURI_SIGNING_PRIVATE_KEY`, `..._PASSWORD`), 공개키를 Variables(`TAURI_UPDATER_PUBKEY`)에 등록 → v0.1.0, v0.1.1 시험판 두 개를 배포해 앱 내부 업데이트 종단 시험.
4. 코드 서명: Windows(인증서 또는 Azure Trusted Signing), macOS(Developer ID + 공증) 준비.

## 우선순위 2 — 기능 보강
- OCR 연동 안내 강화 또는 선택적 로컬 OCR(예: Tesseract/ocrmypdf 감지) 검토
- 선택적 재정렬 모델(Ollama 기반 교차 점수) 실험
- 노트 편집기: `[[` 입력 시 노트 제목 자동 완성, 논문 인용 삽입
- 지식 그래프: 노드 위치 저장, 대규모(500+ 노드)에서 성능 측정
- 백업 일정 알림(예: 마지막 백업 후 30일), 부분 복원(노트만)
- Zotero Web API 연동(읽기 전용) 설계
- 화면 단위 테스트(vitest + Testing Library) 확대

## 우선순위 3 — 유튜브 시연 준비
- 시연 흐름(README·USER_GUIDE의 “시연 순서”)을 실제 모델로 리허설
- 시연용 화면 녹화 시 데모 연구실 배너(가상 자료) 노출 유지
