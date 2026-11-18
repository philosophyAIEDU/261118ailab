# PROJECT_SPEC — 교수님만의 AI 연구실

## 제품 정의
- 한글 제품명: **교수님만의 AI 연구실** / 영문: **Philosophy Research Hub** / 개발·제공: **필로소피 AI 교육**
- 핵심 메시지: “교수님의 논문과 연구자료를 모으고, 검색하고, 비교하고, 연결하세요.”
- 형태: Windows·macOS 설치형 데스크톱 앱 (Tauri 2)
- 원칙: 로컬 우선, 실제 데이터만 표시(가짜 결과·하드코딩 데모 금지), 존재하지 않는 인용 생성 금지, AI 결과는 검토용 초안

## 명칭 적용 위치
| 위치 | 값 |
|---|---|
| 창 제목·시작 화면·설정 화면 | 교수님만의 AI 연구실 (`tauri.conf.json` `app.windows[0].title`, `Splash`, `Settings`) |
| 설치 프로그램 표시 이름·시작 메뉴·macOS 앱 이름 | `productName` = 교수님만의 AI 연구실 |
| 프로그램 정보 화면 | 교수님만의 AI 연구실 / Philosophy Research Hub, 개발 및 제공: 필로소피 AI 교육 |
| GitHub·기술 문서 | Philosophy Research Hub |
| 데이터 경로·식별자 (이름 변경과 무관하게 고정) | 식별자 `kr.philosophy-ai.research-hub`, 연구실 폴더 `PhilosophyResearchHub` |
| Linux 개발 빌드 패키지명 | Philosophy Research Hub (`tauri.linux.conf.json`, deb 패키지 이름 규칙 때문) |

## 사용자 흐름 (최종 목표 14단계 → 구현 위치)
1. PDF 100편 등록 → 내 논문 (일괄·폴더·끌어다 놓기) — 120편 성능 시험 통과
2. 제목·저자·연도·키워드 자동 추출 → `metadata.rs` (출처 표시, 추정값 확인 필요)
3. 주제별 정리 → 태그·연구 분야·프로젝트
4. 자연어 검색 → AI 연구 검색
5. 실제 논문 근거 답변 → `rag.rs`
6. 출처 클릭 → PDF 해당 쪽 (뷰어 + 근거 문단 표시)
7. 2~10편 비교 → 논문 비교
8. 비교표 → 표 + 근거 쪽
9. 연구 노트 생성·연결 → 연구 노트
10. 지식 그래프 → 지식 그래프
11. 문헌고찰 개요 → 문헌고찰 지원
12. Word·Excel·Markdown 저장 → 내보내기
13. 백업·복원 → 설정 > 연구실 자료·백업
14. 앱 내부 업데이트 → 설정 > 업데이트 / 시작 시 확인

## 완료 기준과 현재 상태
상세한 검증 상태는 [PROJECT_STATUS.md](PROJECT_STATUS.md)에 있다. “검증 필요”는 구현했지만 해당 환경에서 시험하지 못한 항목이다.

## 범위 밖 / 향후
- 클라우드 AI (명시적 동의·비용 안내 후에만, 현재 없음)
- OCR 내장 (현재는 스캔 PDF 감지·안내만)
- Zotero Web API 직접 연동, HWP 지원
- 별도 재정렬(reranker) 모델 (현재 RRF + 논문당 제한)
