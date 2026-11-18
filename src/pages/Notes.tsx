import { ask as confirmDlg } from '@tauri-apps/plugin-dialog';
import { useCallback, useEffect, useRef, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { ExportButtons, PaperPicker } from '../components/common';
import { api, errMsg, newJob, onJob, type Note, type NoteDetail, type Paper, type Project } from '../lib/api';
import { fmtDate } from '../lib/format';
import { useNav } from '../lib/nav';
import { Badge, Button, Card, cx, Empty, Field, inputCls, Modal, PageHeader, Spinner, useToast } from '../lib/ui';

interface Draft { id: string | null; title: string; body: string; project_id: string | null; tags: string; paper_ids: string[]; from_ai_draft?: boolean }
const toDraft = (n: NoteDetail): Draft => ({ id: n.id, title: n.title, body: n.body, project_id: n.project_id, tags: n.tags.join(', '), paper_ids: n.paper_ids });

export default function Notes() {
  const { route, go } = useNav();
  const toast = useToast();
  const [list, setList] = useState<Note[]>([]);
  const [q, setQ] = useState('');
  const [tag, setTag] = useState('');
  const [detail, setDetail] = useState<NoteDetail | null>(null);
  const [d, setD] = useState<Draft | null>(null);
  const [preview, setPreview] = useState(false);
  const [saved, setSaved] = useState<'saved' | 'dirty' | 'saving' | 'error'>('saved');
  const [projects, setProjects] = useState<Project[]>([]);
  const [papers, setPapers] = useState<Paper[]>([]);
  const [pickOpen, setPickOpen] = useState(false);
  const [aiOpen, setAiOpen] = useState(false);
  const timer = useRef<number | null>(null);

  const loadList = useCallback(() => api.notes({ query: q, tag: tag || null }).then(setList), [q, tag]);
  useEffect(() => { const t = setTimeout(loadList, 200); return () => clearTimeout(t); }, [loadList]);
  useEffect(() => { api.projects().then(setProjects); api.papers({ sort: 'title' }).then(setPapers); }, []);
  const openNote = useCallback(async (id: string) => {
    const n = await api.note(id);
    setDetail(n);
    setD(toDraft(n));
    setSaved('saved');
  }, []);
  useEffect(() => {
    if (route.params?.id) openNote(route.params.id);
    if (route.params?.newForPaper) { setD({ id: null, title: '', body: '', project_id: null, tags: '', paper_ids: [route.params.newForPaper] }); setDetail(null); setAiOpen(true); }
  }, [route.params, openNote]);

  const persist = useCallback(async (x: Draft) => {
    if (!x.title.trim()) return;
    setSaved('saving');
    try {
      const n = await api.noteSave(x.id, { title: x.title, body: x.body, project_id: x.project_id, tags: x.tags.split(/[,;]/).map((t) => t.trim()).filter(Boolean), paper_ids: x.paper_ids, from_ai_draft: x.from_ai_draft });
      setDetail(n);
      setD((cur) => (cur && (cur.id === null || cur.id === n.id) ? { ...cur, id: n.id, from_ai_draft: false } : cur));
      setSaved('saved');
      loadList();
    } catch (e) {
      setSaved('error');
      toast(errMsg(e), 'error');
    }
  }, [loadList, toast]);
  // 자동 저장 (입력이 멈추고 0.8초 뒤)
  const change = (patch: Partial<Draft>) => {
    setD((cur) => {
      const next = { ...cur!, ...patch };
      setSaved('dirty');
      if (timer.current) clearTimeout(timer.current);
      timer.current = window.setTimeout(() => persist(next), 800);
      return next;
    });
  };
  const del = async () => {
    if (!d?.id) { setD(null); return; }
    if (!(await confirmDlg(`「${d.title}」 노트를 삭제할까요?`, { title: '노트 삭제', kind: 'warning', okLabel: '삭제', cancelLabel: '취소' }))) return;
    await api.noteDelete(d.id);
    setD(null);
    setDetail(null);
    loadList();
  };
  const allTags = Array.from(new Set(list.flatMap((n) => n.tags)));
  const paperTitle = (id: string) => papers.find((p) => p.id === id)?.title ?? '(삭제된 논문)';

  return (
    <div>
      <PageHeader title="연구 노트" subtitle="논문을 읽으며 떠오른 생각을 Markdown으로 축적하세요. [[노트 제목]]으로 노트끼리 연결하면 백링크가 생깁니다. 자동 저장됩니다."
        actions={<><ExportButtons target={{ target: 'notes' }} formats={['md', 'docx']} name="연구 노트 전체" /><Button kind="primary" onClick={() => { setD({ id: null, title: '', body: '', project_id: null, tags: '', paper_ids: [] }); setDetail(null); }}>새 노트</Button></>} />
      <div className="grid gap-5 lg:grid-cols-[300px_1fr]">
        <div>
          <input className={inputCls} placeholder="노트 검색" value={q} onChange={(e) => setQ(e.target.value)} />
          {allTags.length > 0 && <div className="mt-2 flex flex-wrap gap-1">{allTags.map((t) => <button key={t} onClick={() => setTag(tag === t ? '' : t)}><Badge tone={tag === t ? 'violet' : 'muted'}>#{t}</Badge></button>)}</div>}
          <ul className="mt-3 space-y-1">
            {list.length === 0 && <li className="text-sm text-muted">노트가 없습니다.</li>}
            {list.map((n) => (
              <li key={n.id}>
                <button onClick={() => openNote(n.id)} className={cx('w-full rounded-lg border px-3 py-2 text-left', d?.id === n.id ? 'border-violet/40 bg-violet-soft/50' : 'border-transparent hover:bg-paper')}>
                  <div className="truncate text-sm font-medium">{n.title}</div>
                  <div className="text-[11px] text-muted">{fmtDate(n.updated_at)}{n.paper_ids.length ? ` · 논문 ${n.paper_ids.length}` : ''}{n.origin === 'ai_reviewed' ? ' · AI 초안 검토' : ''}</div>
                </button>
              </li>
            ))}
          </ul>
        </div>
        {!d ? <Empty title="노트를 선택하거나 새로 만드세요" action={<Button onClick={() => setAiOpen(true)}>논문으로 AI 초안 만들기</Button>} /> : (
          <Card>
            <div className="mb-3 flex items-center gap-2">
              <input aria-label="노트 제목" className="min-w-0 flex-1 border-b border-line bg-transparent pb-1 text-lg font-semibold text-navy focus:border-violet focus:outline-none" placeholder="노트 제목" value={d.title} onChange={(e) => change({ title: e.target.value })} />
              <span className="text-xs text-muted">{{ saved: d.id ? '저장됨' : '제목을 입력하면 저장됩니다', dirty: '입력 중…', saving: '저장 중…', error: '저장 실패' }[saved]}</span>
            </div>
            <div className="mb-3 grid gap-3 md:grid-cols-2">
              <Field label="태그 (쉼표로 구분)"><input className={inputCls} value={d.tags} onChange={(e) => change({ tags: e.target.value })} /></Field>
              <Field label="연구 프로젝트">
                <select className={inputCls} value={d.project_id ?? ''} onChange={(e) => change({ project_id: e.target.value || null })}>
                  <option value="">없음</option>{projects.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
                </select>
              </Field>
            </div>
            <div className="mb-2 flex flex-wrap items-center gap-1.5">
              <span className="text-[13px] font-medium text-navy">관련 논문</span>
              {d.paper_ids.map((id) => <span key={id} className="inline-flex items-center gap-1"><button onClick={() => go('papers', { id })}><Badge tone="navy">{paperTitle(id).slice(0, 40)}</Badge></button></span>)}
              <Button kind="ghost" className="px-2 py-0.5 text-xs" onClick={() => setPickOpen(true)}>논문 연결…</Button>
            </div>
            <div className="mb-2 flex gap-1.5">
              <Button kind={preview ? 'secondary' : 'primary'} className="px-2.5 py-1 text-xs" onClick={() => setPreview(false)}>편집</Button>
              <Button kind={preview ? 'primary' : 'secondary'} className="px-2.5 py-1 text-xs" onClick={() => setPreview(true)}>미리보기</Button>
            </div>
            {preview ? <div className="prose-lab min-h-80 rounded-lg border border-line bg-white p-4"><ReactMarkdown remarkPlugins={[remarkGfm]}>{d.body || '_내용 없음_'}</ReactMarkdown></div> :
              <textarea aria-label="노트 본문" className={cx(inputCls, 'min-h-80 font-mono text-[13.5px] leading-relaxed')} value={d.body} onChange={(e) => change({ body: e.target.value })} placeholder={'## 핵심 주장\n\n- \n\n## 내 생각\n\n[[다른 노트 제목]]'} />}
            {detail && (detail.backlinks.length > 0 || detail.outgoing.length > 0 || detail.unresolved.length > 0) && (
              <div className="mt-4 grid gap-3 border-t border-line pt-3 text-sm md:grid-cols-2">
                <div><h4 className="mb-1 text-[13px] font-semibold text-navy">이 노트를 가리키는 노트 (백링크)</h4>{detail.backlinks.length === 0 ? <p className="text-xs text-muted">없음</p> : detail.backlinks.map((b) => <button key={b.id} className="mr-2 text-violet hover:underline" onClick={() => openNote(b.id)}>{b.title}</button>)}</div>
                <div><h4 className="mb-1 text-[13px] font-semibold text-navy">연결한 노트</h4>{detail.outgoing.map((b) => <button key={b.id} className="mr-2 text-violet hover:underline" onClick={() => openNote(b.id)}>{b.title}</button>)}{detail.unresolved.map((u) => <span key={u} className="mr-2 text-xs text-muted" title="해당 제목의 노트가 아직 없습니다">[[{u}]]?</span>)}</div>
              </div>
            )}
            <div className="mt-4 flex justify-between">
              <Button kind="danger" onClick={del}>삭제</Button>
              <div className="flex gap-2">
                <Button onClick={() => setAiOpen(true)}>AI 초안으로 내용 추가</Button>
                {d.id && <ExportButtons target={{ target: 'notes', ids: [d.id] }} formats={['md', 'docx']} name={d.title} />}
              </div>
            </div>
          </Card>
        )}
      </div>
      <Modal open={pickOpen} onClose={() => setPickOpen(false)} title="관련 논문 연결" footer={<Button kind="primary" onClick={() => setPickOpen(false)}>확인</Button>}>
        {d && <PaperPicker selected={d.paper_ids} onChange={(ids) => change({ paper_ids: ids })} />}
      </Modal>
      <AiDraftModal open={aiOpen} onClose={() => setAiOpen(false)} papers={papers} initialPaper={d?.paper_ids[0] ?? route.params?.newForPaper}
        onAccept={(title, body, pid) => {
          setAiOpen(false);
          if (d && d.id) change({ body: `${d.body}\n\n${body}`, paper_ids: Array.from(new Set([...d.paper_ids, pid])) });
          else { const nd: Draft = { id: null, title, body, project_id: d?.project_id ?? null, tags: d?.tags ?? '', paper_ids: [pid], from_ai_draft: true }; setD(nd); setDetail(null); persist(nd); }
          toast('AI 초안을 반영했습니다. 내용을 원문과 대조해 검토해 주세요.', 'ok');
        }} />
    </div>
  );
}

function AiDraftModal({ open, onClose, papers, initialPaper, onAccept }: { open: boolean; onClose: () => void; papers: Paper[]; initialPaper?: string; onAccept: (title: string, body: string, paperId: string) => void }) {
  const toast = useToast();
  const [pid, setPid] = useState(initialPaper ?? '');
  const [focus, setFocus] = useState('');
  const [busy, setBusy] = useState(false);
  const [stream, setStream] = useState('');
  const [draft, setDraft] = useState<{ title: string; body: string; removed_markers: number } | null>(null);
  const job = useRef<string | null>(null);
  useEffect(() => { if (open) { setPid(initialPaper ?? ''); setDraft(null); setStream(''); } }, [open, initialPaper]);
  const run = async () => {
    const id = newJob();
    job.current = id;
    setBusy(true);
    setStream('');
    setDraft(null);
    const un = await onJob(id, (e) => e.stage === 'token' && setStream((s) => s + e.message));
    try {
      const r = await api.noteAiDraft(pid, focus, id);
      setDraft(r);
    } catch (e) {
      toast(errMsg(e), 'error');
    } finally {
      un();
      setBusy(false);
    }
  };
  return (
    <Modal open={open} onClose={() => { if (busy && job.current) api.jobCancel(job.current); onClose(); }} title="AI 연구 노트 초안" wide
      footer={<><Button onClick={onClose}>취소</Button>{draft && <Button kind="primary" onClick={() => onAccept(draft.title, draft.body, pid)}>검토했습니다 — 노트에 저장</Button>}</>}>
      <p className="mb-3 rounded-lg bg-ivory px-3 py-2 text-xs text-muted">AI가 논문 본문 일부만 근거로 초안을 만듭니다. 자동으로 저장되지 않으며, 교수님이 내용을 확인한 뒤 “노트에 저장”을 눌러야 저장됩니다.</p>
      <div className="grid gap-3 md:grid-cols-[1fr_1fr]">
        <Field label="논문">
          <select className={inputCls} value={pid} onChange={(e) => setPid(e.target.value)}>
            <option value="">선택…</option>
            {papers.filter((p) => ['ok', 'partial'].includes(p.text_status)).map((p) => <option key={p.id} value={p.id}>{p.title}</option>)}
          </select>
        </Field>
        <Field label="정리할 관점 (선택)"><input className={inputCls} value={focus} onChange={(e) => setFocus(e.target.value)} placeholder="예: 연구 방법과 내 수업 적용 가능성" /></Field>
      </div>
      <Button className="mt-3" kind="primary" disabled={!pid || busy} onClick={run}>{busy ? '작성 중…' : '초안 만들기'}</Button>
      {busy && <div className="mt-3 rounded-lg border border-line bg-white p-3 text-sm whitespace-pre-wrap text-muted">{stream || <Spinner label="논문 문단을 읽는 중" />}</div>}
      {draft && (
        <div className="mt-4">
          <Badge tone="violet">AI 초안 · 검토 필요</Badge>
          {draft.removed_markers > 0 && <span className="ml-2 text-xs text-muted">존재하지 않는 출처 표기 {draft.removed_markers}개 제거됨</span>}
          <div className="prose-lab mt-2 max-h-[50vh] overflow-y-auto rounded-lg border border-line bg-white p-4"><ReactMarkdown remarkPlugins={[remarkGfm]}>{draft.body}</ReactMarkdown></div>
        </div>
      )}
    </Modal>
  );
}
