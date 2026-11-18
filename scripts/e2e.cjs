// 실제 데스크톱 앱(WebKitGTK)을 WebDriver로 조작하는 종단 간 테스트 (Linux, Xvfb).
// 준비: tauri-driver, WebKitWebDriver, Xvfb, 가짜 Ollama(scripts/fake-ollama.cjs)
// 실행: node scripts/e2e.cjs <앱 실행 파일> <스크린샷 폴더>
const fs = require('fs');
const path = require('path');
const app = path.resolve(process.argv[2]);
const shots = path.resolve(process.argv[3] || 'e2e-shots');
fs.mkdirSync(shots, { recursive: true });
const W = 'http://127.0.0.1:4444';
let sid;
const E = 'element-6066-11e4-a52e-4f735466cecf';
async function wd(method, p, body) {
  const r = await fetch(`${W}${p}`, { method, headers: { 'Content-Type': 'application/json' }, body: body ? JSON.stringify(body) : undefined });
  const j = await r.json();
  if (j.value && j.value.error) throw new Error(`${p}: ${j.value.error} ${j.value.message}`);
  return j.value;
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function waitFor(desc, fn, timeout = 30000) {
  const t0 = Date.now();
  let last;
  while (Date.now() - t0 < timeout) {
    try { const v = await fn(); if (v) return v; } catch (e) { last = e; }
    await sleep(300);
  }
  throw new Error(`시간 초과: ${desc} ${last ? last.message : ''}`);
}
const xp = async (x) => (await wd('POST', `/session/${sid}/element`, { using: 'xpath', value: x }))[E];
const xps = async (x) => (await wd('POST', `/session/${sid}/elements`, { using: 'xpath', value: x })).map((e) => e[E]);
const css = async (c) => (await wd('POST', `/session/${sid}/element`, { using: 'css selector', value: c }))[E];
const click = async (el) => wd('POST', `/session/${sid}/element/${el}/click`, {});
const type = async (el, text) => wd('POST', `/session/${sid}/element/${el}/value`, { text });
const exec = async (script, args = []) => wd('POST', `/session/${sid}/execute/sync`, { script, args });
const bodyText = () => exec('return document.body.innerText');
const btn = (label) => `//button[contains(normalize-space(.), "${label}")]`;
async function shot(name) {
  const b64 = await wd('GET', `/session/${sid}/screenshot`);
  fs.writeFileSync(path.join(shots, `${name}.png`), Buffer.from(b64, 'base64'));
  console.log('  📸', name);
}
async function nav(label) { await click(await xp(`//nav//button[contains(., "${label}")]`)); await sleep(600); }
async function selectOption(selectXpath, value) {
  await click(await xp(`${selectXpath}//option[@value="${value}"]`));
  await sleep(500);
}
const results = [];
async function step(name, fn) {
  process.stdout.write(`▶ ${name}\n`);
  try { await fn(); results.push([name, 'PASS']); } catch (e) { results.push([name, `FAIL: ${e.message}`]); console.log('  ✗', e.message); try { await shot(`FAIL-${name.replace(/\s+/g, '_')}`); } catch {} }
}

(async () => {
  const s = await wd('POST', '/session', { capabilities: { alwaysMatch: { 'tauri:options': { application: app } } } });
  sid = s.sessionId;
  await exec('window.resizeTo && window.resizeTo(1440, 900)');
  await step('앱 시작·제품명 표시', async () => {
    await waitFor('사이드바', async () => (await bodyText()).includes('AI 연구 검색'), 40000);
    const t = await bodyText();
    if (!t.includes('교수님만의 AI 연구실')) throw new Error('제품명 없음');
    const title = await wd('GET', `/session/${sid}/title`);
    if (title !== '교수님만의 AI 연구실') throw new Error(`창 제목: ${title}`);
    await shot('01-dashboard-empty');
  });
  await step('데모 연구실 열기 (PDF 12편 등록)', async () => {
    await click(await xp(btn('데모 연구실 열어 보기')));
    await waitFor('데모 등록', async () => (await bodyText()).includes('데모 연구실입니다'), 180000);
    await sleep(800);
    await shot('02-dashboard-demo');
  });
  await step('내 논문 목록·상세', async () => {
    await nav('내 논문');
    await waitFor('목록', async () => (await bodyText()).includes('12편'));
    await click(await xp(`//li//button[contains(., "구조방정식 분석")]`));
    await waitFor('상세', async () => (await bodyText()).includes('서지정보') || (await bodyText()).includes('논문 정보'));
    await sleep(500);
    await shot('03-papers');
  });
  await step('PDF 뷰어 열기·쪽 이동', async () => {
    await click(await xp(btn('PDF 열기')));
    await waitFor('캔버스', async () => (await exec('return document.querySelectorAll(".pdf-page canvas").length')) > 0, 30000);
    await waitFor('텍스트 레이어', async () => (await exec('return document.querySelectorAll(".textLayer span").length')) > 20, 30000);
    await sleep(800);
    await shot('04-viewer');
  });
  await step('AI 모델 선택·의미 검색 색인', async () => {
    await nav('설정');
    await waitFor('AI 탭', async () => (await bodyText()).includes('역할별 모델 선택'), 20000);
    await selectOption('//label[contains(., "답변 생성 모델")]//select', 'fake-chat:latest');
    await selectOption('//label[contains(., "임베딩 모델")]//select', 'fake-embed:latest');
    await waitFor('색인 버튼', async () => !(await exec(`return document.evaluate('${btn('의미 검색 색인 만들기')}', document, null, 9, null).singleNodeValue.disabled`)), 15000);
    await click(await xp(btn('의미 검색 색인 만들기')));
    await waitFor('색인 완료', async () => (await bodyText()).includes('색인이 필요한 문단: 0'), 120000);
    await shot('05-settings-ai');
  });
  await step('AI 연구 검색 (근거·인용 검증)', async () => {
    await nav('AI 연구 검색');
    const ta = await css('textarea');
    await type(ta, '내가 저장한 논문 중 생성형 AI와 비판적 사고의 관계를 다룬 연구를 찾아줘.');
    await click(await xp(`//button[normalize-space(.)="검색"]`));
    await waitFor('답변', async () => (await bodyText()).includes('답변의 근거'), 60000);
    const t = await bodyText();
    if (t.includes('S42')) throw new Error('존재하지 않는 출처가 표시됨');
    if (!t.includes('인용 검증')) throw new Error('검증 안내 없음');
    await shot('06-search');
    await click(await xp(`//button[contains(., "쪽 열기")]`));
    await waitFor('출처 쪽 하이라이트', async () => (await exec('return document.querySelectorAll(".textLayer span.src-hit").length')) > 0, 30000);
    await sleep(600);
    await shot('07-viewer-source');
  });
  await step('논문 비교표', async () => {
    await nav('논문 비교');
    await waitFor('목록', async () => (await xps('//label[input[@type="checkbox"] and .//span[contains(., "가상 데모")]]')).length >= 3);
    const boxes = await xps('//label[input[@type="checkbox"] and .//span[contains(., "가상 데모")]]//input');
    for (const b of boxes.slice(0, 3)) await click(b);
    await click(await xp(btn('비교표 만들기')));
    await waitFor('비교표', async () => (await bodyText()).includes('원문에서 확인되지 않음'), 90000);
    await sleep(500);
    await shot('08-compare');
  });
  await step('지식 그래프', async () => {
    await nav('지식 그래프');
    await waitFor('노드', async () => (await exec('return document.querySelectorAll("svg circle").length')) > 10, 20000);
    await sleep(2500);
    await shot('09-graph');
  });
  await step('연구 노트', async () => {
    await nav('연구 노트');
    await click(await xp(`//li//button[contains(., "연구 공백 메모")]`));
    await waitFor('노트 열림', async () => String(await exec('return document.querySelector("textarea[aria-label=\\"노트 본문\\"]")?.value ?? ""')).includes('연구 공백 후보'));
    await shot('10-notes');
  });
  await step('문헌고찰 개요', async () => {
    await nav('문헌고찰 지원');
    const boxes = await waitFor('목록', async () => { const b = await xps('//label[input[@type="checkbox"] and .//span[contains(., "가상 데모")]]//input'); return b.length >= 3 && b; });
    for (const b of boxes.slice(0, 4)) await click(b);
    await click(await xp(btn('개요 초안 만들기')));
    await waitFor('초안', async () => (await bodyText()).includes('참고문헌 (등록된 서지정보 기준)'), 90000);
    await shot('11-review');
  });
  if (process.env.E2E_GEMINI) await step('Gemini API 선택·동의·키 저장·검색·사용량', async () => {
    await nav('설정');
    await waitFor('AI 탭', async () => (await bodyText()).includes('답변 생성 AI 선택'), 20000);
    await click(await xp(`//button[@role="radio" and contains(., "Gemini API")]`));
    await waitFor('동의 창', async () => (await bodyText()).includes('Google로 전송되는 것'));
    await shot('13-gemini-consent');
    await click(await xp(`//label[contains(., "위 내용을 확인했으며")]//input`));
    await click(await xp(btn('동의하고 Gemini 사용')));
    await waitFor('키 입력', async () => (await xps('//input[@type="password"]')).length > 0);
    await type(await xp('//input[@type="password"]'), 'AIzaFAKE_E2E_TEST_KEY_1234567890');
    await click(await xp(btn('확인 후 저장')));
    await waitFor('키 저장', async () => (await bodyText()).includes('저장된 키'), 20000);
    if ((await bodyText()).includes('E2E_TEST_KEY')) throw new Error('키 원문이 화면에 노출됨');
    await sleep(800);
    await shot('14-gemini-settings');
    await nav('AI 연구 검색');
    await waitFor('배너', async () => (await bodyText()).includes('Gemini API 사용 중'));
    const ta = await css('textarea');
    await exec('arguments[0].value=""', [{ [E]: ta }]);
    await type(ta, '비판적 사고 성향 향상 효과');
    await click(await xp(`//button[normalize-space(.)="검색"]`));
    await waitFor('Gemini 답변', async () => (await bodyText()).includes('(Gemini API)'), 60000);
    await waitFor('사용량 1회', async () => /오늘 무료 사용 1 \/ 500회/.test(await bodyText()), 10000);
    await shot('15-gemini-search');
    await nav('설정');
    await waitFor('AI 탭', async () => (await bodyText()).includes('답변 생성 AI 선택'), 20000);
    await click(await xp(`//button[@role="radio" and contains(., "로컬 AI (Ollama)")]`));
    await waitFor('로컬 전환', async () => (await bodyText()).includes('로컬 AI로 전환했습니다'), 10000);
  });
  await step('설정 — 보안·업데이트 화면', async () => {
    await nav('설정');
    await click(await xp(`//button[@role="tab" and contains(., "업데이트")]`));
    await click(await xp(btn('지금 업데이트 확인')));
    await waitFor('업데이트 응답', async () => { const t = await bodyText(); return t.includes('서명 공개키') || t.includes('최신 버전') || t.includes('업데이트 서버'); }, 30000);
    await shot('12-update');
  });
  await wd('DELETE', `/session/${sid}`);
  console.log('\n결과');
  for (const [n, r] of results) console.log(`${r.startsWith('PASS') ? '✅' : '❌'} ${n} — ${r}`);
  fs.writeFileSync(path.join(shots, 'results.json'), JSON.stringify(results, null, 1));
  process.exit(results.every((r) => r[1] === 'PASS') ? 0 : 1);
})().catch(async (e) => { console.error(e); process.exit(2); });
