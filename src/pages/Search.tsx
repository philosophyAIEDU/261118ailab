import { useEffect, useRef, useState } from 'react';
import { CloudBanner } from '../components/AiProvider';
import { ExportButtons, PaperPicker, SourceCard } from '../components/common';
import { api, errMsg, newJob, onJob, type AskResult, type Hit, type Mode, type Project, type TodayUsage } from '../lib/api';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { fmtDate } from '../lib/format';
import { useNav, useOpenSource } from '../lib/nav';
import { Badge, Button, Card, cx, Empty, inputBase, inputCls, Modal, PageHeader, Spinner, useToast } from '../lib/ui';

const MODE_LABEL: Record<Mode, string> = { hybrid: '하이브리드', semantic: '의미 검색', keyword: '키워드' };
const EXAMPLE = '내가 저장한 논문 중 생성형 AI와 비판적 사고의 관계를 다룬 연구를 찾아줘.';

/** AI 답변(Markdown)을 표시하되 [S1] 표기는 클릭 가능한 출처 칩으로, 외부 링크·이미지는 표시하지 않는다 */
export function AnswerText({ text, labels, onCite }: { text: string; labels: Set<string>; onCite: (l: string) => void }) {
  const md = text.replace(/\[(S\d+)\]/g, (_, l) => (labels.has(l) ? `[${l}](#cite-${l})` : ''));
  return (
    <div className="prose-lab text-[15px]">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={{
        a: ({ href, children }) => href?.startsWith('#cite-') ? (
          <button onClick={() => onCite(href.slice(6))} className="mx-0.5 rounded bg-navy/10 px-1 text-[12px] font-semibold text-navy no-underline hover:bg-navy hover:text-white">{children}</button>
        ) : <span>{children}</span>,
        img: () => null,
      }}>{md}</ReactMarkdown>
    </div>
  );
}

export default function Search() {
  const { route, go } = useNav();
  const toast = useToast();
  const open = useOpenSource();
  const [q, setQ] = useState('');
  const [mode, setMode] = useState<Mode>('hybrid');
  const [aiOn, setAiOn] = useState(true);
  const [scope, setScope] = useState<string[]>(route.params?.scope ?? []);
  const [projects, setProjects] = useState<Project[]>([]);
  const [pickOpen, setPickOpen] = useState(false);
  const [stream, setStream] = useState('');
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<AskResult | null>(null);
  const [hits, setHits] = useState<{ hits: Hit[]; notice: string | null } | null>(null);
  const [history, setHistory] = useState<AskResult[]>([]);
  const [settings, setSettings] = useState<any>(null);
  const [usage, setUsage] = useState<TodayUsage | null>(null);
  const [notice, setNotice] = useState('');
  const job = useRef<string | null>(null);

  useEffect(() => {
    api.searchesRecent(30).then((h) => {
      setHistory(h);
      if (route.params?.resultId) setResult(h.find((x) => x.id === route.params!.resultId) ?? null);
    });
    api.projects().then(setProjects);
    api.settings().then(setSettings);
    api.geminiUsage().then(setUsage).catch(() => {});
  }, [route.params?.resultId]);

  const run = async () => {
    if (!q.trim()) return;
    setResult(null);
    setHits(null);
    setStream('');
    setBusy(true);
    try {
      if (!aiOn) {
        const r = await api.searchPassages(q, mode, 20, scope);
        setHits({ hits: r.hits, notice: r.notice });
      } else {
        const id = newJob();
        job.current = id;
        setNotice('');
        const un = await onJob(id, (e) => { if (e.stage === 'token') setStream((s) => s + e.message); if (e.stage === 'notice') setNotice(e.message); });
        try {
          const r = await api.ask({ question: q, mode, paper_ids: scope, top_k: settings?.top_k ?? 8 }, id);
          setResult(r);
          setHistory((h) => [r, ...h.filter((x) => x.id !== r.id)].slice(0, 30));
        } finally {
          un();
          api.geminiUsage().then(setUsage).catch(() => {});
        }
      }
    } catch (e) {
      if ((e as any)?.kind !== 'cancelled') toast(errMsg(e), 'error');
    } finally {
      setBusy(false);
      setStream('');
    }
  };
  const cite = (label: string) => document.getElementById(`src-${label}`)?.scrollIntoView({ behavior: 'smooth', block: 'center' });

  return (
    <div>
      <PageHeader title="AI 연구 검색" subtitle="내 연구 라이브러리에 자연어로 질문하세요. AI는 실제로 검색된 원문만 근거로 답하고, 모든 출처는 PDF 쪽으로 연결됩니다." />
      <CloudBanner settings={settings} usage={usage} />
      <Card className="mb-5">
        <textarea className={cx(inputCls, 'min-h-20 text-[15px]')} placeholder={EXAMPLE} value={q} onChange={(e) => setQ(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) run(); }} />
        <div className="mt-3 flex flex-wrap items-center gap-2">
          <div className="inline-flex rounded-lg border border-line p-0.5" role="radiogroup" aria-label="검색 방식">
            {(['hybrid', 'semantic', 'keyword'] as Mode[]).map((m) => (
              <button key={m} role="radio" aria-checked={mode === m} onClick={() => setMode(m)} className={cx('rounded-md px-3 py-1 text-sm', mode === m ? 'bg-navy text-white' : 'text-navy')}>{MODE_LABEL[m]}</button>
            ))}
          </div>
          <label className="inline-flex items-center gap-1.5 text-sm"><input type="checkbox" className="accent-[#7155A4]" checked={aiOn} onChange={(e) => setAiOn(e.target.checked)} />AI 답변 생성</label>
          <select className={cx(inputBase, 'w-auto py-1.5')} aria-label="검색 범위" value="" onChange={(e) => { const p = projects.find((x) => x.id === e.target.value); if (p) setScope(p.paper_ids); }}>
            <option value="">프로젝트 논문으로 범위 지정…</option>
            {projects.map((p) => <option key={p.id} value={p.id}>{p.name} ({p.paper_ids.length}편)</option>)}
          </select>
          <Button kind="ghost" onClick={() => setPickOpen(true)}>{scope.length ? `범위: ${scope.length}편` : '범위: 전체 논문'}</Button>
          {scope.length > 0 && <button className="text-xs text-muted underline" onClick={() => setScope([])}>전체로</button>}
          <div className="flex-1" />
          {busy && aiOn ? <Button onClick={() => job.current && api.jobCancel(job.current)}>중단</Button> : null}
          <Button kind="primary" disabled={busy || !q.trim()} onClick={run}>{busy ? '검색 중…' : '검색'}</Button>
        </div>
        {aiOn && settings && settings.provider !== 'gemini' && !settings.chat_model && <p className="mt-2 text-xs text-[#8a6a2c]">답변 생성 모델이 선택되지 않았습니다. 원문 검색 결과만 표시됩니다. <button className="underline" onClick={() => go('settings', { tab: 'ai' })}>AI 모델 설정</button></p>}
      </Card>

      <div className="grid gap-5 lg:grid-cols-[1fr_260px]">
        <div>
          {busy && aiOn && (
            <Card className="mb-4" title={<span className="flex items-center gap-2">AI 답변 생성 중 <Badge tone="gold">검증 전 초안</Badge></span>}>
              {notice && <p className="mb-2 text-xs text-[#7a5b22]">{notice}</p>}
              {stream ? <p className="text-sm whitespace-pre-wrap text-muted">{stream}</p> : <Spinner label="관련 문단을 찾는 중" />}
            </Card>
          )}
          {busy && !aiOn && <Spinner label="찾는 중" />}
          {result && <ResultView r={result} onCite={cite} />}
          {hits && (
            <div className="space-y-3">
              {hits.notice && <p className="rounded-lg bg-gold-soft px-3 py-2 text-xs text-[#7a5b22]">{hits.notice}</p>}
              {hits.hits.length === 0 ? <Empty title="일치하는 원문이 없습니다">다른 표현으로 검색하거나 의미 검색 색인을 만들어 보세요.</Empty> :
                hits.hits.map((h, i) => (
                  <SourceCard key={h.chunk_id} s={{ label: `${i + 1}`, chunk_id: h.chunk_id, paper_id: h.paper_id, paper_title: h.paper_title, authors: h.authors, year: h.year, page: h.page, text: h.text, suspicious: false }}
                    badge={<>{h.keyword_rank && <Badge tone="teal">키워드 {h.keyword_rank}위</Badge>}{h.vector_rank && <Badge tone="violet">의미 {h.vector_rank}위</Badge>}</>} />
                ))}
            </div>
          )}
          {!busy && !result && !hits && <Empty title="질문을 입력해 보세요">예: “{EXAMPLE}”</Empty>}
        </div>
        <aside>
          <h3 className="mb-2 text-sm font-semibold text-navy">최근 AI 검색</h3>
          {history.length === 0 ? <p className="text-xs text-muted">기록이 없습니다.</p> : (
            <ul className="space-y-1">
              {history.map((h) => (
                <li key={h.id} className="group flex items-start gap-1">
                  <button className={cx('min-w-0 flex-1 rounded px-1.5 py-1 text-left text-[13px] hover:bg-paper', result?.id === h.id && 'bg-paper font-medium')} onClick={() => { setResult(h); setHits(null); setQ(h.question); }}>
                    <span className="line-clamp-2">{h.question}</span><span className="text-[11px] text-muted">{fmtDate(h.created_at)}</span>
                  </button>
                  <button aria-label="기록 삭제" className="invisible px-1 text-muted group-hover:visible hover:text-danger" onClick={() => api.searchDelete(h.id).then(() => { setHistory((x) => x.filter((y) => y.id !== h.id)); if (result?.id === h.id) setResult(null); })}>×</button>
                </li>
              ))}
            </ul>
          )}
        </aside>
      </div>
      <Modal open={pickOpen} onClose={() => setPickOpen(false)} title="검색 범위 — 논문 선택" footer={<Button kind="primary" onClick={() => setPickOpen(false)}>확인</Button>}>
        <PaperPicker selected={scope} onChange={setScope} />
      </Modal>
    </div>
  );

  function ResultView({ r, onCite }: { r: AskResult; onCite: (l: string) => void }) {
    const labels = new Set(r.citations.map((c) => c.label));
    return (
      <div className="space-y-4">
        <Card title={<span className="flex flex-wrap items-center gap-2">AI 답변 {r.status === 'answered' ? <Badge tone="violet">AI 생성 · 교수님 검토 필요</Badge> : r.status === 'insufficient' ? <Badge tone="gold">근거 부족</Badge> : <Badge tone="danger">AI 사용 불가</Badge>}<Badge>{MODE_LABEL[r.mode_used]}</Badge>{r.model && <Badge>{r.model}</Badge>}</span>}
          action={<ExportButtons target={{ target: 'search', id: r.id }} formats={['docx', 'md']} name={`AI검색-${r.question.slice(0, 20)}`} />}>
          <p className="mb-2 text-sm text-muted">질문: {r.question}</p>
          {r.status === 'answered' ? <AnswerText text={r.answer} labels={labels} onCite={onCite} /> : <p className="text-[15px] font-medium text-navy">{r.answer}</p>}
          {r.notice && <p className="mt-3 rounded-lg bg-gold-soft px-3 py-2 text-xs text-[#7a5b22]">{r.notice}</p>}
          {(r.removed_markers > 0 || r.removed_references.length > 0) && (
            <p className="mt-3 rounded-lg bg-ivory px-3 py-2 text-xs text-muted">인용 검증: 실제 검색 결과에 없는 출처 표기 {r.removed_markers}개{r.removed_references.length ? `와 등록 자료에서 확인되지 않은 인용 ${r.removed_references.join(', ')}` : ''}를 제거했습니다.</p>
          )}
        </Card>
        {r.citations.length > 0 && (
          <div>
            <h3 className="mb-2 text-sm font-semibold text-navy">답변의 근거 (실제 원문)</h3>
            <div className="space-y-2.5">
              {r.citations.map((c) => (
                <SourceCard key={c.label} s={c.source} badge={c.support === 'verified' ? <Badge tone="teal" title="답변 문장과 원문의 어휘가 충분히 겹칩니다">근거 확인</Badge> : <Badge tone="gold" title="실제 원문이지만 답변 문장과의 대응이 약합니다. 직접 확인해 주세요.">원문 확인 필요</Badge>} />
              ))}
            </div>
          </div>
        )}
        {r.other_sources.length > 0 && (
          <div>
            <h3 className="mb-2 text-sm font-semibold text-navy">{r.status === 'answered' ? '함께 검색된 원문 (답변에서 인용되지 않음)' : '검색된 원문'}</h3>
            <div className="space-y-2">{r.other_sources.map((s) => <SourceCard key={s.chunk_id} s={s} compact />)}</div>
          </div>
        )}
        {r.status === 'answered' && r.citations.length > 0 && <Button kind="ghost" className="text-xs" onClick={() => open(r.citations[0].source.paper_id, r.citations[0].source.page, r.citations[0].source.text)}>첫 번째 근거 PDF 열기 →</Button>}
      </div>
    );
  }
}
