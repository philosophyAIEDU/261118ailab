// PDF.js 문자 지도(CMap)·표준 글꼴을 public/pdfjs로 복사한다 (한글 PDF 표시에 필요, 오프라인 동작)
const fs = require('fs');
const path = require('path');
const root = path.join(__dirname, '..');
const src = path.join(root, 'node_modules/pdfjs-dist');
const dst = path.join(root, 'public/pdfjs');
for (const d of ['cmaps', 'standard_fonts']) {
  fs.cpSync(path.join(src, d), path.join(dst, d), { recursive: true });
}
console.log('PDF.js 자료 복사 완료');
