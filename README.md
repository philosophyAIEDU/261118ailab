# 교수님만의 AI 연구실 (Philosophy Research Hub)

> 교수님의 논문과 연구자료를 모으고, 검색하고, 비교하고, 연결하세요.
> 흩어진 연구자료를 평생 활용할 수 있는 나만의 지식 자산으로 만듭니다.

**개발 및 제공: 필로소피 AI 교육** · *AI로 성장하고, 그 성장을 나눈다.*

교수님의 컴퓨터에 설치해 사용하는 **데스크톱 연구 프로그램**(Windows·macOS)입니다.
논문 PDF를 등록하면 쪽 번호와 함께 검색할 수 있게 정리하고, 이 컴퓨터에서 실행되는 로컬 AI(Ollama)로
실제 논문을 근거로 답하고, 논문을 비교하고, 연구 노트와 지식 그래프로 연결합니다.

## 주요 기능

| 기능 | 설명 |
|---|---|
| 논문 등록 | PDF·DOCX·TXT·Markdown·BibTeX·RIS, 끌어다 놓기·폴더 단위 일괄 등록, 중복 검사(파일 해시·DOI·제목), 진행률, 실패 파일 재처리 |
| 서지정보 추출 | 제목·저자·연도·DOI·초록·키워드를 **문서에 실제로 있는 정보만** 추출하고 필드별 출처 표시 (추정값은 “확인 필요”) |
| PDF 뷰어 | 쪽 이동, 확대·축소, 문서 내 찾기, 하이라이트·메모·북마크, AI 출처 쪽 이동과 근거 문단 표시 |
| AI 연구 검색 | 키워드(BM25)·의미(임베딩)·하이브리드(RRF) 검색 → 근거 기반 답변 → **인용 검증**(존재하지 않는 출처·인용 제거) → PDF 쪽 연결 |
| 논문 비교 | 2~10편, 연구 목적~후속 연구 제안 10개 항목, 칸마다 원문 쪽 근거, 확인 안 되면 “원문에서 확인되지 않음” |
| 연구 노트 | Markdown, 자동 저장, 논문·프로젝트 연결, 태그, 검색, `[[노트 제목]]` 링크와 백링크, AI 초안(검토 후 저장) |
| 지식 그래프 | 논문·노트·주제·핵심 개념·프로젝트, 실제 인용(본문 확인)과 AI 제안(점선) 구분, 필터·검색·확대 |
| 문헌고찰 지원 | 규칙 기반 주제 분류, 연구 흐름·방법론·결과·한계, 연구 공백 **후보**, 개요 초안, 참고문헌(등록된 서지정보만) |
| 외부 도구 | Zotero BibTeX/RIS 가져오기(내부 DB 미접근), Obsidian 내보내기(YAML·태그·링크·프로젝트 폴더, 덮어쓰기 없음) |
| 내보내기 | Word·Excel·CSV·Markdown·TXT·BibTeX — 출처와 검증 상태 포함 |
| 백업·복원 | 전체 연구실 백업(PDF 포함 선택), 해시·무결성 검증, 선택적 암호화(Argon2id + XChaCha20-Poly1305), 충돌 확인 후 복원 |
| 자동 업데이트 | Tauri 2 공식 Updater, 안정/베타 채널, 서명 검증, 설치 전 DB 안전 사본 |
| 데모 연구실 | 유튜브 시연용 **가상** 논문 12편·프로젝트·노트 (실제 논문 아님 명시, 실제 연구실과 분리) |

## 설치 (교수님용)

1. [GitHub Releases](https://github.com/philosophyAIEDU/261118ailab/releases)에서 설치 파일을 내려받습니다.
   - Windows: `교수님만의 AI 연구실_x.y.z_x64-setup.exe`
   - macOS: `교수님만의 AI 연구실_x.y.z_universal.dmg`
2. AI 기능을 쓰려면 [Ollama](https://ollama.com/download)를 설치하고, 프로그램의 **설정 → AI 모델**에서 추천 모델을 내려받습니다.
   AI 없이도 논문 등록·키워드 검색·PDF 보기·노트·내보내기·백업은 사용할 수 있습니다.

자세한 사용법은 [사용 설명서](docs/USER_GUIDE.md)를 참고하세요.

> ⚠️ 현재 설치 파일은 코드 서명·공증 전입니다. Windows SmartScreen이나 macOS Gatekeeper 경고가 나타날 수 있습니다.
> Windows·macOS 실기기 검증 상태는 [PROJECT_STATUS.md](PROJECT_STATUS.md)를 확인하세요.

## 자료는 어디에 저장되나요?

- 연구실 폴더(기본값): `문서/PhilosophyResearchHub` — PDF 사본, 데이터베이스(메타데이터·노트·색인·벡터·설정), 자동 백업
- 프로그램 파일과 분리되어 있어 **업데이트·재설치·제거 후에도 유지**됩니다.
- 원본 PDF는 읽기만 하며 수정·삭제하지 않습니다.
- 연구자료는 외부로 전송되지 않습니다. 인터넷은 AI 모델 내려받기(Ollama)와 업데이트 확인(GitHub)에만 쓰입니다.

## 개발

```bash
npm ci
npm run tauri dev          # 개발 실행 (Linux는 webkit2gtk-4.1 등 필요)
npm run build              # 화면 빌드 + 타입 검사
npm test                   # 화면 단위 테스트 (vitest)
cargo test -p prh-core     # 핵심 로직 테스트 (한글 PDF, AI 파이프라인, 백업 등)
npx tauri build            # 설치 파일 빌드
```

| 경로 | 내용 |
|---|---|
| `crates/core` | 핵심 로직 (Tauri 비의존): DB·추출·검색·RAG·비교·노트·그래프·내보내기·백업 |
| `src-tauri` | 데스크톱 앱: 명령 계층, 연구실 위치, 업데이트 |
| `src` | React + TypeScript + Tailwind 화면 |
| `demo` | 가상 데모 논문 명세와 PDF |
| `scripts` | 데모 PDF 생성, PDF.js 자료 복사, 가짜 Ollama(테스트용), E2E 테스트 |

문서: [PROJECT_SPEC](PROJECT_SPEC.md) · [ARCHITECTURE](ARCHITECTURE.md) · [DATA_MODEL](DATA_MODEL.md) · [RAG_DESIGN](RAG_DESIGN.md) ·
[SECURITY_CHECKLIST](SECURITY_CHECKLIST.md) · [PROJECT_STATUS](PROJECT_STATUS.md) · [NEXT_TASK](NEXT_TASK.md) ·
[TEST_RESULTS](TEST_RESULTS.md) · [RELEASE_CHECKLIST](RELEASE_CHECKLIST.md) · [CHANGELOG](CHANGELOG.md)

## 라이선스

MIT © 필로소피 AI 교육. 사양 진단·모델 추천 방식은 같은 팀의 [Philosophy Local AI](https://github.com/philosophyAIEDU/261110philosophylocalai)를 참고했습니다.
