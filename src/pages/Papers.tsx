import { getCurrentWebview } from '@tauri-apps/api/webview';
import { ask as confirmDlg } from '@tauri-apps/plugin-dialog';
import { useCallback, useEffect, useRef, useState } from 'react';
import { api, errMsg, newJob, onJob, type ImportOutcome, type Paper } from '../lib/api';
import { pickDocuments, pickFolder } from '../lib/dialogs';
import { authorsShort, fmtBytes, fmtDate, SOURCE_LABEL, STATUS_LABEL } from '../lib/format';
import { useNav, useOpenSource } from '../lib/nav';
import { Badge, Button, Card, cx, Empty, Field, inputBase, inputCls, PageHeader, Progress, Spinner, useToast } from '../lib/ui';

const statusTone = (s: string) => (s === 'ok' ? 'teal' : s === 'partial' || s === 'metadata_only' ? 'gold' : s === 'pending' ? 'muted' : 'danger') as any;

export function useImporter(onDone?: () => void) {
  const toast = useToast();
  const [prog, setProg] = useState<{ done: number; total: number; message: string } | null>(null);
  const [results, setResults] = useState<ImportOutcome[] | null>(null);
  const jobRef = useRef<string | null>(null);
  const start = useCallback(async (paths: string[]) => {
    if (!paths.length) return;
    const job = newJob();
    jobRef.current = job;
    setResults(null);
    setProg({ done: 0, total: paths.length, message: '준비 중' });
    const un = await onJob(job, (e) => {
      if (e.stage === 'progress') setProg({ done: e.done, total: e.total, message: e.message || '처리 중' });
      if (e.stage === 'done') {
        setProg(null);
        setResults(e.data as ImportOutcome[]);
        const r = e.data as ImportOutcome[];
        const added = r.filter((x) => x.status === 'added').length;
        toast(`등록 ${added}건 · 중복 ${r.filter((x) => x.status === 'duplicate').length}건 · 실패 ${r.filter((x) => x.status === 'failed').length}건`, 'ok');
        un();
        onDone?.();
      }
    });
    try {
      await api.importStart(paths, job);
    } catch (e) {
      un();
      setProg(null);
      toast(errMsg(e), 'error');
    }
  }, [onDone, toast]);
  const cancel = () => jobRef.current && api.jobCancel(jobRef.current);
  return { start, prog, results, setResults, cancel };
}

export function ImportPanel({ imp }: { imp: ReturnType<typeof useImporter> }) {
  const { go } = useNav();
  const toast = useToast();
  if (!imp.prog && !imp.results) return null;
  const retry = async (id: string) => {
    try {
      const p = await api.paperReprocess(id);
      toast(p.text_status === 'failed' ? `다시 처리했지만 실패했습니다: ${p.status_detail}` : '다시 처리했습니다', p.text_status === 'failed' ? 'error' : 'ok');
    } catch (e) {
      toast(errMsg(e), 'error');
    }
  };
  return (
    <Card className="mb-5" title={imp.prog ? '논문 등록 중' : '등록 결과'} action={imp.prog ? <Button kind="ghost" onClick={imp.cancel}>중단</Button> : <Button kind="ghost" onClick={() => imp.setResults(null)}>닫기</Button>}>
      {imp.prog && <Progress done={imp.prog.done} total={imp.prog.total} label={`${imp.prog.done}/${imp.prog.total} · ${imp.prog.message}`} />}
      {imp.results && (
        <ul className="scroll-thin max-h-64 divide-y divide-line/70 overflow-y-auto text-sm">
          {imp.results.map((r, i) => (
            <li key={i} className="flex items-start gap-2 py-1.5">
              <Badge tone={r.status === 'added' || r.status === 'updated' ? 'teal' : r.status === 'duplicate' ? 'gold' : 'danger'}>{{ added: '등록', updated: '서지 연결', duplicate: '중복', failed: '실패' }[r.status]}</Badge>
              <div className="min-w-0 flex-1">
                <div className="truncate font-medium">{r.file_name}</div>
                <div className="text-xs text-muted">{r.message}{r.warnings.length ? ` · ${r.warnings.join(' ')}` : ''}</div>
              </div>
              {r.paper_id && r.status === 'failed' && <Button kind="ghost" className="text-xs" onClick={() => retry(r.paper_id!)}>다시 처리</Button>}
              {r.paper_id && r.status !== 'failed' && <Button kind="ghost" className="text-xs" onClick={() => go('papers', { id: r.paper_id })}>보기</Button>}
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}

function SourceTag({ s }: { s?: string }) {
  if (!s) return null;
  return <Badge tone={s === 'text_guess' || s === 'file_name' ? 'gold' : s === 'user' ? 'violet' : 'muted'}>{SOURCE_LABEL[s] ?? s}</Badge>;
}

function PaperDetail({ id, onChanged, onClose }: { id: string; onChanged: () => void; onClose: () => void }) {
  const toast = useToast();
  const open = useOpenSource();
  const { go } = useNav();
  const [d, setD] = useState<Awaited<ReturnType<typeof api.paper>> | null>(null);
  const [f, setF] = useState<any>(null);
  const [saving, setSaving] = useState(false);
  const load = useCallback(() => api.paper(id).then((r) => {
    setD(r);
    const p = r.paper;
    setF({ title: p.title, authors: p.authors.join('; '), year: p.year ?? '', journal: p.journal, doi: p.doi, abstract: p.abstract, keywords: p.keywords.join(', '), field: p.field, memo: p.memo, tags: p.tags.join(', ') });
  }).catch((e) => toast(errMsg(e), 'error')), [id, toast]);
  useEffect(() => { load(); }, [load]);
  if (!d || !f) return <Card><Spinner label="불러오는 중" /></Card>;
  const p = d.paper;
  const split = (s: string, sep: RegExp) => s.split(sep).map((x) => x.trim()).filter(Boolean);
  const save = async () => {
    setSaving(true);
    try {
      const y = String(f.year).trim();
      await api.paperUpdate(id, { title: f.title, authors: split(f.authors, /;/), year: y ? Number(y) : null, journal: f.journal, doi: f.doi, abstract: f.abstract, keywords: split(f.keywords, /[,;]/), field: f.field, memo: f.memo, tags: split(f.tags, /[,;]/) });
      toast('서지정보를 저장했습니다', 'ok');
      await load();
      onChanged();
    } catch (e) {
      toast(errMsg(e), 'error');
    } finally {
      setSaving(false);
    }
  };
  const del = async () => {
    const ok = await confirmDlg(`「${p.title}」을(를) 연구실에서 삭제할까요?\n\n연구실 안의 기록과 사본만 삭제되며, 교수님 컴퓨터의 원본 파일은 삭제되지 않습니다.`, { title: '논문 삭제', kind: 'warning', okLabel: '삭제', cancelLabel: '취소' });
    if (!ok) return;
    await api.paperDelete([id]);
    toast('삭제했습니다 (원본 파일은 그대로입니다)', 'ok');
    onChanged();
    onClose();
  };
  const reprocess = async () => {
    try {
      const r = await api.paperReprocess(id);
      toast(`다시 처리했습니다: ${STATUS_LABEL[r.text_status]}`, r.text_status === 'failed' ? 'error' : 'ok');
      load();
      onChanged();
    } catch (e) {
      toast(errMsg(e), 'error');
    }
  };
  const inp = (k: string, label: string, src?: string, area?: boolean) => (
    <Field label={label} hint={src ? <span className="inline-flex items-center gap-1">출처: <SourceTag s={p.meta_source[src]} /></span> : undefined}>
      {area ? <textarea className={cx(inputCls, 'min-h-24')} value={f[k]} onChange={(e) => setF({ ...f, [k]: e.target.value })} /> : <input className={inputCls} value={f[k]} onChange={(e) => setF({ ...f, [k]: e.target.value })} />}
    </Field>
  );
  return (
    <Card title="논문 정보" action={<button className="text-muted hover:text-navy" onClick={onClose} aria-label="닫기">×</button>}>
      <div className="mb-3 flex flex-wrap gap-1.5">
        <Badge tone={statusTone(p.text_status)}>{STATUS_LABEL[p.text_status] ?? p.text_status}</Badge>
        {p.page_count != null && <Badge>{p.page_count}쪽</Badge>}
        <Badge>{p.file_kind.toUpperCase() || '서지'}</Badge>
        {p.file_size > 0 && <Badge>{fmtBytes(p.file_size)}</Badge>}
        <Badge>검색 단위 {p.chunk_count}개</Badge>
      </div>
      {p.status_detail && p.text_status !== 'ok' && <p className="mb-3 rounded-lg bg-gold-soft px-3 py-2 text-xs text-[#7a5b22]">{p.status_detail}{p.text_status === 'no_text' && ' (예: Adobe Acrobat의 "텍스트 인식", macOS 미리보기, OCRmyPDF 등으로 문자 인식 후 다시 등록하세요.)'}</p>}
      <div className="mb-3 flex flex-wrap gap-2">
        {p.file_kind === 'pdf' && <Button kind="primary" onClick={() => open(p.id, 1)}>PDF 열기</Button>}
        <Button onClick={() => go('notes', { newForPaper: p.id })}>연구 노트 쓰기</Button>
        <Button onClick={() => go('search', { scope: [p.id] })}>이 논문에 질문</Button>
        {p.file_path && <Button kind="ghost" onClick={reprocess}>본문 다시 추출</Button>}
      </div>
      <div className="space-y-3">
        {inp('title', '제목', 'title')}
        {inp('authors', '저자 (세미콜론 ; 으로 구분)', 'authors')}
        <div className="grid grid-cols-[110px_1fr] gap-3">{inp('year', '출판 연도', 'year')}{inp('journal', '학술지', 'journal')}</div>
        {inp('doi', 'DOI', 'doi')}
        {inp('keywords', '키워드 (쉼표로 구분)', 'keywords')}
        {inp('abstract', '초록', 'abstract', true)}
        <div className="grid grid-cols-2 gap-3">{inp('field', '연구 분야')}{inp('tags', '태그 (쉼표로 구분)')}</div>
        {inp('memo', '개인 메모', undefined, true)}
      </div>
      <p className="mt-2 text-xs text-muted">자동 추출 정보는 문서에 실제로 있는 내용만 가져옵니다. "자동 추정 — 확인 필요" 표시는 배치로 추정한 값이니 확인해 주세요.</p>
      <div className="mt-3 flex justify-between">
        <Button kind="danger" onClick={del}>연구실에서 삭제</Button>
        <Button kind="primary" disabled={saving} onClick={save}>{saving ? '저장 중…' : '저장'}</Button>
      </div>
      {(d.citations.length > 0 || d.notes.length > 0 || d.projects.length > 0) && (
        <div className="mt-5 space-y-3 border-t border-line pt-4 text-sm">
          {d.citations.length > 0 && (
            <div>
              <h4 className="mb-1 font-semibold text-navy">인용 관계 (본문에서 확인됨)</h4>
              <ul className="space-y-1">{d.citations.map((c: any, i: number) => <li key={i}><Badge tone="navy">{c.direction === 'out' ? '이 논문이 인용' : '이 논문을 인용'}</Badge> <button className="hover:text-violet" onClick={() => go('papers', { id: c.paper_id })}>{c.title}</button> <span className="text-xs text-muted">({c.evidence === 'doi' ? 'DOI' : '제목'} 일치{c.page ? `, ${c.page}쪽` : ''})</span></li>)}</ul>
            </div>
          )}
          {d.notes.length > 0 && <div><h4 className="mb-1 font-semibold text-navy">연결된 노트</h4>{d.notes.map((n) => <button key={n.id} className="mr-2 text-violet hover:underline" onClick={() => go('notes', { id: n.id })}>{n.title}</button>)}</div>}
          {d.projects.length > 0 && <div><h4 className="mb-1 font-semibold text-navy">프로젝트</h4>{d.projects.map((x) => <button key={x.id} className="mr-2 text-violet hover:underline" onClick={() => go('projects', { id: x.id })}>{x.name}</button>)}</div>}
        </div>
      )}
    </Card>
  );
}

export default function Papers() {
  const { route } = useNav();
  const [papers, setPapers] = useState<Paper[] | null>(null);
  const [q, setQ] = useState('');
  const [tag, setTag] = useState('');
  const [status, setStatus] = useState('');
  const [sort, setSort] = useState('recent');
  const [tags, setTags] = useState<[string, number][]>([]);
  const [sel, setSel] = useState<string | null>(route.params?.id ?? null);
  const [drag, setDrag] = useState(false);
  const load = useCallback(() => {
    api.papers({ query: q, tag: tag || null, status: status || null, sort }).then(setPapers);
    api.tags().then(setTags);
  }, [q, tag, status, sort]);
  const imp = useImporter(load);
  useEffect(() => {
    const t = setTimeout(load, 200);
    return () => clearTimeout(t);
  }, [load]);
  useEffect(() => {
    if (route.params?.id) setSel(route.params.id);
  }, [route.params?.id]);
  useEffect(() => {
    if (route.params?.import) pickDocuments().then(imp.start);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // 창으로 파일 끌어다 놓기
  useEffect(() => {
    let un: (() => void) | undefined;
    getCurrentWebview().onDragDropEvent((e) => {
      const t = e.payload.type;
      if (t === 'enter' || t === 'over') setDrag(true);
      else if (t === 'leave') setDrag(false);
      else if (t === 'drop') {
        setDrag(false);
        imp.start(e.payload.paths);
      }
    }).then((f) => (un = f));
    return () => un?.();
  }, [imp.start]);

  return (
    <div>
      <PageHeader title="내 논문" subtitle="PDF·DOCX·TXT·Markdown·BibTeX·RIS 파일을 끌어다 놓거나 선택해 등록하세요. 원본 파일은 수정하지 않습니다."
        actions={<><Button onClick={async () => { const d = await pickFolder('논문이 들어 있는 폴더 선택'); if (d) imp.start([d]); }}>폴더 등록</Button><Button kind="primary" onClick={async () => imp.start(await pickDocuments())}>파일 등록</Button></>} />
      {drag && <div className="mb-5 rounded-xl border-2 border-dashed border-violet bg-violet-soft py-8 text-center font-semibold text-violet">여기에 놓으면 연구실에 등록합니다</div>}
      <ImportPanel imp={imp} />
      <div className="mb-4 flex flex-wrap gap-2">
        <input className={cx(inputBase, 'max-w-md')} placeholder="제목·저자·키워드·본문 검색" value={q} onChange={(e) => setQ(e.target.value)} />
        <select className={cx(inputBase, 'w-auto')} value={tag} onChange={(e) => setTag(e.target.value)} aria-label="태그">
          <option value="">모든 태그</option>
          {tags.map(([t, n]) => <option key={t} value={t}>{t} ({n})</option>)}
        </select>
        <select className={cx(inputBase, 'w-auto')} value={status} onChange={(e) => setStatus(e.target.value)} aria-label="상태">
          <option value="">모든 상태</option>
          {Object.entries(STATUS_LABEL).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
        </select>
        <select className={cx(inputBase, 'w-auto')} value={sort} onChange={(e) => setSort(e.target.value)} aria-label="정렬">
          <option value="recent">최근 등록순</option><option value="title">제목순</option><option value="year">연도순</option>
        </select>
      </div>
      <div className={cx('grid gap-5', sel ? 'lg:grid-cols-[1fr_440px]' : '')}>
        <div>
          {!papers ? <Spinner label="불러오는 중" /> : papers.length === 0 ? (
            <Empty title={q || tag || status ? '조건에 맞는 논문이 없습니다' : '아직 등록한 논문이 없습니다'} action={!q && <Button kind="primary" onClick={async () => imp.start(await pickDocuments())}>파일 등록</Button>}>
              {!q && '논문 PDF를 이 창으로 끌어다 놓아도 됩니다.'}
            </Empty>
          ) : (
            <div className="overflow-hidden rounded-xl border border-line bg-paper">
              <div className="border-b border-line px-4 py-2 text-xs text-muted">{papers.length}편</div>
              <ul className="divide-y divide-line/70">
                {papers.map((p) => (
                  <li key={p.id}>
                    <button onClick={() => setSel(p.id)} className={cx('block w-full px-4 py-3 text-left hover:bg-ivory/70', sel === p.id && 'bg-violet-soft/50')}>
                      <div className="flex items-start gap-2">
                        <span className="min-w-0 flex-1 font-medium text-ink">{p.title}</span>
                        <Badge tone={statusTone(p.text_status)}>{STATUS_LABEL[p.text_status] ?? p.text_status}</Badge>
                      </div>
                      <div className="mt-0.5 text-xs text-muted">{authorsShort(p.authors)} · {p.year ?? '연도 미상'}{p.journal ? ` · ${p.journal}` : ''} · 등록 {fmtDate(p.created_at)}</div>
                      {(p.tags.length > 0 || p.keywords.length > 0) && (
                        <div className="mt-1.5 flex flex-wrap gap-1">{p.tags.map((t) => <Badge key={t} tone="violet">#{t}</Badge>)}{p.keywords.slice(0, 5).map((k) => <Badge key={k}>{k}</Badge>)}</div>
                      )}
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
        {sel && <div className="lg:sticky lg:top-4 lg:self-start"><PaperDetail key={sel} id={sel} onChanged={load} onClose={() => setSel(null)} /></div>}
      </div>
    </div>
  );
}
