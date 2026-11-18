import * as pdfjs from 'pdfjs-dist';
import workerUrl from 'pdfjs-dist/build/pdf.worker.min.mjs?url';
import { useCallback, useEffect, useRef, useState } from 'react';
import { api, errMsg, type Annotation, type Paper } from '../lib/api';
import { useNav } from '../lib/nav';
import { Badge, Button, cx, inputBase, inputCls, Spinner, Tabs, useToast } from '../lib/ui';

pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

const squash = (s: string) => s.replace(/\s+/g, '').toLowerCase();

interface PageState { width: number; height: number }

export default function Viewer() {
  const { route, go } = useNav();
  const toast = useToast();
  const paperId: string = route.params?.paperId;
  const startPage: number = Math.max(1, Number(route.params?.page ?? 1));
  const highlight: string | undefined = route.params?.highlight;
  const [paper, setPaper] = useState<Paper | null>(null);
  const [doc, setDoc] = useState<pdfjs.PDFDocumentProxy | null>(null);
  const [err, setErr] = useState('');
  const [scale, setScale] = useState(1.25);
  const [base, setBase] = useState<PageState | null>(null);
  const [cur, setCur] = useState(startPage);
  const [pageInput, setPageInput] = useState(String(startPage));
  const [anns, setAnns] = useState<Annotation[]>([]);
  const [tab, setTab] = useState<'notes' | 'find' | 'source'>(highlight ? 'source' : 'notes');
  const [find, setFind] = useState('');
  const [findRes, setFindRes] = useState<{ page: number; snippet: string }[] | null>(null);
  const [findTerm, setFindTerm] = useState('');
  const [sel, setSel] = useState<{ page: number; text: string; rects: Annotation['rects']; x: number; y: number } | null>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const pageEls = useRef<Map<number, HTMLDivElement>>(new Map());
  const textCache = useRef<Map<number, string>>(new Map());
  const scrolledToStart = useRef(false);

  useEffect(() => {
    let task: pdfjs.PDFDocumentLoadingTask | null = null;
    (async () => {
      try {
        const r = await api.paper(paperId);
        setPaper(r.paper);
        const buf = await api.paperFile(paperId);
        task = pdfjs.getDocument({ data: new Uint8Array(buf), cMapUrl: '/pdfjs/cmaps/', cMapPacked: true, standardFontDataUrl: '/pdfjs/standard_fonts/', enableXfa: false });
        const d = await task.promise;
        const p1 = await d.getPage(1);
        const vp = p1.getViewport({ scale: 1 });
        setBase({ width: vp.width, height: vp.height });
        setDoc(d);
        setAnns(await api.annotations(paperId));
      } catch (e) {
        setErr(errMsg(e));
      }
    })();
    return () => {
      task?.destroy();
    };
  }, [paperId]);

  const pageText = useCallback(async (n: number) => {
    if (!doc) return '';
    const c = textCache.current.get(n);
    if (c !== undefined) return c;
    const page = await doc.getPage(n);
    const tc = await page.getTextContent();
    const t = tc.items.map((i: any) => i.str).join(' ');
    textCache.current.set(n, t);
    return t;
  }, [doc]);

  const scrollTo = useCallback((n: number) => {
    const el = pageEls.current.get(n);
    if (el && scroller.current) scroller.current.scrollTo({ top: el.offsetTop - 12 });
  }, []);

  // 처음 열 때 지정한 쪽으로 이동
  useEffect(() => {
    if (doc && base && !scrolledToStart.current) {
      scrolledToStart.current = true;
      requestAnimationFrame(() => scrollTo(Math.min(startPage, doc.numPages)));
    }
  }, [doc, base, startPage, scrollTo]);

  // 현재 쪽 추적
  const onScroll = () => {
    const sc = scroller.current;
    if (!sc) return;
    const mid = sc.scrollTop + sc.clientHeight / 3;
    let best = 1;
    pageEls.current.forEach((el, n) => {
      if (el.offsetTop <= mid) best = Math.max(best, n);
    });
    if (best !== cur) {
      setCur(best);
      setPageInput(String(best));
    }
  };

  const runFind = async () => {
    if (!doc || !find.trim()) return;
    const q = squash(find);
    const out: { page: number; snippet: string }[] = [];
    for (let n = 1; n <= doc.numPages; n++) {
      const t = await pageText(n);
      const idx = squash(t).indexOf(q);
      if (idx >= 0) {
        const raw = t.replace(/\s+/g, ' ');
        const i2 = raw.toLowerCase().indexOf(find.trim().toLowerCase());
        const at = i2 >= 0 ? i2 : 0;
        out.push({ page: n, snippet: raw.slice(Math.max(0, at - 40), at + 80) });
      }
    }
    setFindTerm(find.trim());
    setFindRes(out);
    if (out[0]) scrollTo(out[0].page);
  };

  const onMouseUp = () => {
    const s = window.getSelection();
    if (!s || s.isCollapsed || !s.toString().trim()) return setSel(null);
    const range = s.getRangeAt(0);
    const pageEl = (range.startContainer.parentElement as HTMLElement | null)?.closest('.pdf-page') as HTMLElement | null;
    if (!pageEl) return setSel(null);
    const n = Number(pageEl.dataset.page);
    const pr = pageEl.getBoundingClientRect();
    const rects = Array.from(range.getClientRects())
      .filter((r) => r.width > 1 && r.height > 1)
      .map((r) => ({ x: (r.left - pr.left) / pr.width, y: (r.top - pr.top) / pr.height, w: r.width / pr.width, h: r.height / pr.height }));
    const last = range.getBoundingClientRect();
    setSel({ page: n, text: s.toString().trim().slice(0, 2000), rects, x: last.right, y: last.bottom });
  };

  const addAnn = async (kind: Annotation['kind'], page: number, text = '', rects: Annotation['rects'] = [], note = '') => {
    try {
      const a = await api.annotationAdd({ paper_id: paperId, page, kind, text, note, color: kind === 'highlight' ? 'violet' : '', rects });
      setAnns((x) => [...x, a].sort((p, q) => p.page - q.page));
      setSel(null);
      window.getSelection()?.removeAllRanges();
      toast(kind === 'bookmark' ? `${page}쪽을 북마크했습니다` : kind === 'highlight' ? '하이라이트를 저장했습니다' : '메모를 추가했습니다', 'ok');
    } catch (e) {
      toast(errMsg(e), 'error');
    }
  };
  const bookmarked = anns.some((a) => a.kind === 'bookmark' && a.page === cur);

  if (err) return <div className="rounded-xl border border-line bg-paper p-6"><p className="font-semibold text-danger">PDF를 열 수 없습니다</p><p className="mt-1 text-sm text-muted">{err}</p><Button className="mt-3" onClick={() => go('papers', { id: paperId })}>논문 정보로 돌아가기</Button></div>;

  return (
    <div className="-mx-8 -my-7 flex h-[100vh] flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b border-line bg-paper px-4 py-2">
        <Button kind="ghost" onClick={() => go('papers', { id: paperId })}>← 논문 정보</Button>
        <div className="min-w-0 flex-1 truncate text-sm font-semibold text-navy" title={paper?.title}>{paper?.title ?? '불러오는 중…'}</div>
        <div className="flex items-center gap-1 text-sm">
          <Button kind="ghost" className="px-2" aria-label="이전 쪽" onClick={() => scrollTo(Math.max(1, cur - 1))}>‹</Button>
          <input aria-label="쪽 번호" className={cx(inputBase, 'w-14 px-1.5 py-1 text-center')} value={pageInput} onChange={(e) => setPageInput(e.target.value)} onKeyDown={(e) => e.key === 'Enter' && doc && scrollTo(Math.min(doc.numPages, Math.max(1, Number(pageInput) || 1)))} />
          <span className="text-muted">/ {doc?.numPages ?? '–'}</span>
          <Button kind="ghost" className="px-2" aria-label="다음 쪽" onClick={() => doc && scrollTo(Math.min(doc.numPages, cur + 1))}>›</Button>
        </div>
        <div className="flex items-center gap-1">
          <Button kind="ghost" className="px-2" aria-label="축소" onClick={() => setScale((s) => Math.max(0.5, +(s - 0.15).toFixed(2)))}>−</Button>
          <span className="w-12 text-center text-xs text-muted">{Math.round(scale * 100)}%</span>
          <Button kind="ghost" className="px-2" aria-label="확대" onClick={() => setScale((s) => Math.min(3, +(s + 0.15).toFixed(2)))}>＋</Button>
          <Button kind="ghost" className="text-xs" onClick={() => base && scroller.current && setScale(+((scroller.current.clientWidth - 48) / base.width).toFixed(2))}>폭 맞춤</Button>
        </div>
        <Button kind={bookmarked ? 'gold' : 'secondary'} className="text-xs" onClick={() => (bookmarked ? api.annotationDelete(anns.find((a) => a.kind === 'bookmark' && a.page === cur)!.id).then(() => setAnns((x) => x.filter((a) => !(a.kind === 'bookmark' && a.page === cur)))) : addAnn('bookmark', cur))}>
          {bookmarked ? '★ 북마크됨' : '☆ 북마크'}
        </Button>
      </div>
      <div className="flex min-h-0 flex-1">
        <div ref={scroller} onScroll={onScroll} onMouseUp={onMouseUp} className="scroll-thin relative min-w-0 flex-1 overflow-auto bg-[#e9e4d8] py-4">
          {!doc || !base ? <div className="p-8"><Spinner label="PDF를 여는 중" /></div> :
            Array.from({ length: doc.numPages }, (_, i) => i + 1).map((n) => (
              <PdfPage key={`${n}-${scale}`} doc={doc} n={n} scale={scale} base={base} register={(el) => (el ? pageEls.current.set(n, el) : pageEls.current.delete(n))}
                anns={anns.filter((a) => a.page === n && a.kind === 'highlight')} sourceText={n === startPage ? highlight : undefined} findTerm={findTerm} eager={Math.abs(n - startPage) <= 1} />
            ))}
          {sel && (
            <div className="fixed z-40 flex gap-1 rounded-lg border border-line bg-paper p-1 shadow" style={{ left: Math.min(sel.x, window.innerWidth - 260), top: sel.y + 6 }}>
              <Button kind="primary" className="px-2.5 py-1 text-xs" onClick={() => addAnn('highlight', sel.page, sel.text, sel.rects)}>하이라이트</Button>
              <Button className="px-2.5 py-1 text-xs" onClick={() => { const note = window.prompt('메모 내용', ''); if (note !== null) addAnn('memo', sel.page, sel.text, sel.rects, note); }}>메모</Button>
              <Button kind="ghost" className="px-2 py-1 text-xs" onClick={() => setSel(null)}>×</Button>
            </div>
          )}
        </div>
        <aside className="scroll-thin w-80 shrink-0 overflow-y-auto border-l border-line bg-paper p-4">
          <Tabs value={tab} onChange={setTab} items={[{ key: 'notes', label: '주석' }, { key: 'find', label: '찾기' }, ...(highlight ? [{ key: 'source' as const, label: 'AI 출처' }] : [])]} />
          {tab === 'source' && highlight && (
            <div className="text-sm">
              <p className="mb-2 text-xs text-muted">AI 답변이 근거로 사용한 원문입니다. PDF {startPage}쪽에서 일치하는 부분을 노란색으로 표시했습니다.</p>
              <Button className="mb-2 text-xs" onClick={() => scrollTo(startPage)}>{startPage}쪽으로 이동</Button>
              <p className="rounded-lg border-l-2 border-gold bg-gold-soft/60 p-2.5 text-[13px]">{highlight}</p>
            </div>
          )}
          {tab === 'find' && (
            <div>
              <div className="flex gap-1.5"><input className={inputCls} placeholder="PDF 안에서 찾기" value={find} onChange={(e) => setFind(e.target.value)} onKeyDown={(e) => e.key === 'Enter' && runFind()} /><Button onClick={runFind}>찾기</Button></div>
              {findRes && (findRes.length === 0 ? <p className="mt-3 text-sm text-muted">찾지 못했습니다. (스캔 PDF는 글자가 없어 찾을 수 없습니다)</p> : (
                <ul className="mt-3 space-y-1.5">{findRes.map((r) => <li key={r.page}><button className="w-full rounded-lg border border-line p-2 text-left text-xs hover:border-teal" onClick={() => scrollTo(r.page)}><Badge tone="teal">{r.page}쪽</Badge> {r.snippet}</button></li>)}</ul>
              ))}
            </div>
          )}
          {tab === 'notes' && (
            <div>
              <p className="mb-3 text-xs text-muted">글자를 드래그해 선택하면 하이라이트나 메모를 남길 수 있습니다. 주석은 연구실에만 저장되며 PDF 원본은 바뀌지 않습니다.</p>
              {anns.length === 0 && <p className="text-sm text-muted">아직 주석이 없습니다.</p>}
              <ul className="space-y-2">
                {anns.map((a) => (
                  <li key={a.id} className="rounded-lg border border-line p-2.5 text-sm">
                    <div className="mb-1 flex items-center gap-1.5">
                      <Badge tone={a.kind === 'highlight' ? 'violet' : a.kind === 'memo' ? 'teal' : 'gold'}>{{ highlight: '하이라이트', memo: '메모', bookmark: '북마크' }[a.kind]}</Badge>
                      <button className="text-xs text-navy hover:underline" onClick={() => scrollTo(a.page)}>{a.page}쪽</button>
                      <button className="ml-auto text-xs text-muted hover:text-danger" onClick={() => api.annotationDelete(a.id).then(() => setAnns((x) => x.filter((y) => y.id !== a.id)))}>삭제</button>
                    </div>
                    {a.text && <p className="line-clamp-4 border-l-2 border-violet/40 pl-2 text-xs text-[#444]">{a.text}</p>}
                    {a.kind !== 'bookmark' && (
                      <textarea className={cx(inputCls, 'mt-1.5 min-h-12 text-xs')} placeholder="메모" defaultValue={a.note} onBlur={(e) => e.target.value !== a.note && api.annotationUpdate(a.id, e.target.value)} />
                    )}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}

function PdfPage({ doc, n, scale, base, register, anns, sourceText, findTerm, eager }: {
  doc: pdfjs.PDFDocumentProxy; n: number; scale: number; base: PageState; register: (el: HTMLDivElement | null) => void;
  anns: Annotation[]; sourceText?: string; findTerm: string; eager: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const textDiv = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(eager);
  const [size, setSize] = useState({ w: base.width * scale, h: base.height * scale });
  const [rendered, setRendered] = useState(false);
  useEffect(() => {
    register(ref.current);
    return () => register(null);
  }, [register]);
  useEffect(() => {
    if (visible || !ref.current) return;
    const io = new IntersectionObserver((es) => es.some((e) => e.isIntersecting) && setVisible(true), { rootMargin: '600px 0px' });
    io.observe(ref.current);
    return () => io.disconnect();
  }, [visible]);
  useEffect(() => {
    if (!visible) return;
    let cancelled = false;
    let renderTask: any;
    (async () => {
      const page = await doc.getPage(n);
      const vp = page.getViewport({ scale });
      setSize({ w: vp.width, h: vp.height });
      const c = canvas.current!;
      const dpr = window.devicePixelRatio || 1;
      c.width = Math.floor(vp.width * dpr);
      c.height = Math.floor(vp.height * dpr);
      c.style.width = `${vp.width}px`;
      c.style.height = `${vp.height}px`;
      renderTask = page.render({ canvas: c, canvasContext: c.getContext('2d')!, viewport: vp, transform: dpr !== 1 ? [dpr, 0, 0, dpr, 0, 0] : undefined } as any);
      await renderTask.promise.catch(() => {});
      if (cancelled) return;
      const tl = textDiv.current!;
      tl.replaceChildren();
      const text = new pdfjs.TextLayer({ textContentSource: await page.getTextContent(), container: tl, viewport: vp });
      await text.render();
      if (!cancelled) setRendered(true);
    })();
    return () => {
      cancelled = true;
      renderTask?.cancel?.();
    };
  }, [visible, doc, n, scale]);
  // 출처 문단·찾기 결과 표시
  useEffect(() => {
    if (!rendered || !textDiv.current) return;
    const spans = Array.from(textDiv.current.querySelectorAll('span')) as HTMLSpanElement[];
    const src = sourceText ? squash(sourceText) : '';
    const fq = findTerm ? squash(findTerm) : '';
    let first: HTMLSpanElement | null = null;
    for (const s of spans) {
      const t = squash(s.textContent ?? '');
      s.classList.remove('src-hit', 'find-hit');
      if (src && t.length >= 2 && src.includes(t)) {
        s.classList.add('src-hit');
        first ??= s;
      }
      if (fq && t.includes(fq)) s.classList.add('find-hit');
    }
    if (first) first.scrollIntoView({ block: 'center' });
  }, [rendered, sourceText, findTerm]);
  return (
    <div ref={ref} data-page={n} className="pdf-page" style={{ width: size.w, height: size.h, ['--total-scale-factor' as any]: scale }}>
      <canvas ref={canvas} />
      <div ref={textDiv} className="textLayer" />
      {anns.flatMap((a) => a.rects.map((r, i) => <div key={a.id + i} className="hl" style={{ left: `${r.x * 100}%`, top: `${r.y * 100}%`, width: `${r.w * 100}%`, height: `${r.h * 100}%` }} title={a.note || a.text} />))}
      {!rendered && <div className="absolute inset-0 flex items-center justify-center text-xs text-muted">{n}쪽</div>}
    </div>
  );
}
