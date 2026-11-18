// 개발·테스트 전용 가짜 Ollama 서버 (127.0.0.1). 실제 AI가 아니며 배포물에 포함되지 않는다.
// 사용법: node scripts/fake-ollama.cjs [포트=11434]
const http = require('http');
const port = Number(process.argv[2] || 11434);
const tokens = (t) => (t.toLowerCase().match(/[a-z0-9]+|[가-힣]{1}/g) || []);
function embed(t) {
  const v = new Array(64).fill(0);
  const ts = tokens(t);
  for (let i = 0; i < ts.length - 1; i++) {
    const g = ts[i] + ts[i + 1];
    let h = 7;
    for (const c of g) h = (h * 31 + c.codePointAt(0)) >>> 0;
    v[h % 64] += 1;
  }
  return v;
}
function firstSentence(user, label) {
  const part = user.split(`<source id="${label}"`)[1];
  if (!part) return '';
  const body = part.split('\n').slice(1).join(' ').split('</source>')[0];
  const m = body.match(/[^.]*\./);
  return (m ? m[0] : body.slice(0, 120)).trim();
}
http.createServer((req, res) => {
  let b = '';
  req.on('data', (c) => (b += c));
  req.on('end', () => {
    const body = b ? JSON.parse(b) : {};
    const send = (o, code = 200) => { res.writeHead(code, { 'Content-Type': 'application/json' }); res.end(typeof o === 'string' ? o : JSON.stringify(o)); };
    if (req.url === '/api/version') return send({ version: '0.0.0-fake' });
    if (req.url === '/api/tags') return send({ models: [{ name: 'fake-chat:latest', size: 1000, details: { family: 'gemma3', parameter_size: '1B' } }, { name: 'fake-embed:latest', size: 500, details: { family: 'bert' } }] });
    if (req.url === '/api/ps') return send({ models: [] });
    if (req.url === '/api/embed') return send({ embeddings: body.input.map(embed) });
    if (req.url === '/api/chat') {
      const user = body.messages[body.messages.length - 1].content;
      let reply;
      if (body.format === 'json') {
        const s1 = firstSentence(user, 'S1');
        const s2 = firstSentence(user, 'S2') || s1;
        reply = JSON.stringify({ purpose: { value: s1, sources: ['S1'] }, method: { value: s2, sources: ['S2'] }, findings: { value: s2, sources: ['S2'] }, sample_size: { value: '999명', sources: ['S99'] } });
      } else {
        const a = firstSentence(user, 'S1');
        const c = firstSentence(user, 'S2');
        reply = `## 요약\n\n${a} [S1]\n\n${c} [S2]\n\n(가짜 AI 응답 — 테스트용) 존재하지 않는 근거 [S42]와 (Park, 2019) 인용은 검증 단계에서 제거되어야 합니다.`;
      }
      res.writeHead(200, { 'Content-Type': 'application/x-ndjson' });
      for (let i = 0; i < reply.length; i += 12) res.write(JSON.stringify({ message: { role: 'assistant', content: reply.slice(i, i + 12) }, done: false }) + '\n');
      res.end(JSON.stringify({ done: true, eval_count: 10, eval_duration: 1e7 }) + '\n');
      return;
    }
    if (req.url === '/api/generate') return send({ done: true });
    send({ error: 'not found' }, 404);
  });
}).listen(port, '127.0.0.1', () => console.log(`가짜 Ollama: http://127.0.0.1:${port}`));
