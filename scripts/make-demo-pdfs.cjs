// 데모(가상) 논문 PDF 생성기. demo/demo-papers.json → demo/papers/*.pdf
// 사용법: node scripts/make-demo-pdfs.cjs   (Playwright + Chromium, 한글 글꼴 필요)
const fs = require('fs');
const path = require('path');
const { chromium } = require('playwright');

const root = path.join(__dirname, '..');
const spec = JSON.parse(fs.readFileSync(path.join(root, 'demo/demo-papers.json'), 'utf8'));
const out = path.join(root, 'demo/papers');
fs.mkdirSync(out, { recursive: true });
const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;');

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage();
  for (const p of spec.papers) {
    const sections = p.sections.map(([h, t]) => `<h2>${esc(h)}</h2><p>${esc(t)}</p>`).join('');
    const html = `<!doctype html><html lang="ko"><head><meta charset="utf-8"><title>${esc(p.title)}</title>
<style>
 body{font-family:'Noto Serif CJK KR','Noto Sans CJK KR',serif;font-size:11pt;line-height:1.75;margin:0;color:#111}
 .banner{border:1.5px solid #7155A4;color:#7155A4;padding:6px 10px;font-size:9.5pt;margin-bottom:14px}
 .journal{font-size:9pt;color:#555}
 h1{font-size:17pt;line-height:1.4;margin:10px 0 6px}
 .authors{font-size:10.5pt;margin-bottom:14px}
 .abs{background:#f4f1ea;padding:10px 14px;font-size:10pt}
 h2{font-size:12.5pt;margin:18px 0 4px}
 p{margin:0 0 8px;text-align:justify}
 .refs li{font-size:9.5pt}
</style></head><body>
<div class="banner">※ 이 문서는 「교수님만의 AI 연구실」 시연을 위해 만든 가상 자료입니다. 실제 논문, 실제 저자, 실제 학술지가 아니며 수치도 모두 가상입니다.</div>
<div class="journal">${esc(p.journal)} · ${p.year} (가상)</div>
<h1>${esc(p.title)}</h1>
<div class="authors">${p.authors.map(esc).join(', ')}</div>
<div class="abs"><b>국문초록</b><br>${esc(p.abstract)}<br><br>주제어: ${p.keywords.map(esc).join(', ')}</div>
${sections}
<h2>참고문헌</h2><ul class="refs">${p.references.map((r) => `<li>${esc(r)}</li>`).join('')}</ul>
</body></html>`;
    await page.setContent(html, { waitUntil: 'load' });
    await page.pdf({
      path: path.join(out, `${p.slug}.pdf`),
      format: 'A4',
      margin: { top: '22mm', bottom: '22mm', left: '22mm', right: '22mm' },
      displayHeaderFooter: true,
      headerTemplate: '<div></div>',
      footerTemplate: '<div style="font-size:8px;width:100%;text-align:center;color:#777;font-family:Noto Sans CJK KR">가상 데모 자료 — 실제 논문이 아닙니다 · <span class="pageNumber"></span> / <span class="totalPages"></span></div>',
    });
    console.log('작성:', p.slug);
  }
  await browser.close();
  fs.copyFileSync(path.join(root, 'demo/demo-papers.json'), path.join(out, 'demo-papers.json'));
})();
