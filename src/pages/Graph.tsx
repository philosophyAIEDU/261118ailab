import { forceCenter, forceCollide, forceLink, forceManyBody, forceSimulation, forceX, forceY, type SimulationLinkDatum, type SimulationNodeDatum } from 'd3-force';
import { useEffect, useMemo, useRef, useState } from 'react';
import { api, errMsg, type GEdge, type GNode } from '../lib/api';
import { useNav } from '../lib/nav';
import { Badge, Button, Card, cx, Empty, inputBase, PageHeader, Spinner, useToast } from '../lib/ui';

type N = GNode & SimulationNodeDatum;
type L = Omit<GEdge, 'source' | 'target'> & SimulationLinkDatum<N>;

const NODE: Record<GNode['kind'], { label: string; color: string; r: number }> = {
  paper: { label: '논문', color: '#17263D', r: 9 },
  note: { label: '연구 노트', color: '#4D8D88', r: 7 },
  project: { label: '프로젝트', color: '#C7A05A', r: 11 },
  topic: { label: '연구 주제', color: '#7155A4', r: 7 },
  concept: { label: '핵심 개념', color: '#9a86c4', r: 6 },
};
const EDGE: Record<GEdge['kind'], { label: string; color: string; dash?: string; width: number }> = {
  citation: { label: '실제 인용 (본문 확인)', color: '#17263D', width: 2.2 },
  user: { label: '교수님 연결', color: '#4D8D88', width: 1.6 },
  ai_suggested: { label: 'AI 제안 (확인 전)', color: '#C7A05A', dash: '6 4', width: 2 },
  topic: { label: '동일 주제', color: '#b7a8d8', width: 1 },
  keyword: { label: '공통 키워드', color: '#cfc6b3', width: 1 },
};

export default function Graph() {
  const { go } = useNav();
  const toast = useToast();
  const [data, setData] = useState<{ nodes: GNode[]; edges: GEdge[] } | null>(null);
  const [nodeOn, setNodeOn] = useState<Record<string, boolean>>({ paper: true, note: true, project: true, topic: true, concept: true });
  const [edgeOn, setEdgeOn] = useState<Record<string, boolean>>({ citation: true, user: true, ai_suggested: true, topic: true, keyword: true });
  const [q, setQ] = useState('');
  const [sel, setSel] = useState<{ node?: N; edge?: L } | null>(null);
  const [view, setView] = useState({ x: 0, y: 0, k: 1 });
  const [, setTick] = useState(0);
  const sim = useRef<ReturnType<typeof forceSimulation<N>> | null>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const drag = useRef<{ node?: N; pan?: { x: number; y: number; vx: number; vy: number } } | null>(null);
  const W = 1100, H = 680;
  const load = () => api.graph().then(setData).catch((e) => toast(errMsg(e), 'error'));
  useEffect(() => { load(); }, []);

  const { nodes, links } = useMemo(() => {
    if (!data) return { nodes: [] as N[], links: [] as L[] };
    const ns: N[] = data.nodes.filter((n) => nodeOn[n.kind]).map((n) => ({ ...n }));
    const ids = new Set(ns.map((n) => n.id));
    const ls: L[] = data.edges.filter((e) => edgeOn[e.kind] && ids.has(e.source) && ids.has(e.target)).map((e) => ({ ...e }));
    return { nodes: ns, links: ls };
  }, [data, nodeOn, edgeOn]);

  useEffect(() => {
    sim.current?.stop();
    if (!nodes.length) return;
    const s = forceSimulation<N>(nodes)
      .force('link', forceLink<N, L>(links).id((d) => d.id).distance((l) => (l.kind === 'keyword' || l.kind === 'topic' ? 110 : 150)).strength(0.25))
      .force('charge', forceManyBody().strength(-520).distanceMax(600))
      .force('center', forceCenter(W / 2, H / 2))
      .force('collide', forceCollide<N>().radius((d) => NODE[d.kind].r + 16))
      .force('x', forceX(W / 2).strength(0.03))
      .force('y', forceY(H / 2).strength(0.05))
      .on('tick', () => setTick((t) => t + 1));
    sim.current = s;
    return () => { s.stop(); };
  }, [nodes, links]);

  const ql = q.trim().toLowerCase();
  const match = (n: N) => !!ql && n.label.toLowerCase().includes(ql);
  const toWorld = (e: React.MouseEvent) => {
    const r = svgRef.current!.getBoundingClientRect();
    return { x: ((e.clientX - r.left) * (W / r.width) - view.x) / view.k, y: ((e.clientY - r.top) * (H / r.height) - view.y) / view.k };
  };
  const onWheel = (e: React.WheelEvent) => {
    const k = Math.min(4, Math.max(0.3, view.k * (e.deltaY < 0 ? 1.12 : 0.89)));
    setView((v) => ({ ...v, k }));
  };
  const openNode = (n: N) => {
    if (n.kind === 'paper') go('papers', { id: n.ref_id });
    else if (n.kind === 'note') go('notes', { id: n.ref_id });
    else if (n.kind === 'project') go('projects', { id: n.ref_id });
    else go('papers', {});
  };
  const suggest = async () => {
    try {
      const n = await api.graphSuggest();
      toast(n ? `AI가 관련성 ${n}건을 제안했습니다. 점선으로 표시됩니다 — 확인 전까지는 제안일 뿐입니다.` : '새로 제안할 관련성이 없습니다.', 'ok');
      load();
    } catch (e) {
      toast(errMsg(e), 'error');
    }
  };
  const linkId = (l: L) => l.id.startsWith('link:') ? l.id.slice(5) : null;
  const neighbors = (n: N) => links.filter((l) => (l.source as N).id === n.id || (l.target as N).id === n.id);

  return (
    <div className="mx-auto max-w-[1500px]">
      <PageHeader title="지식 그래프" subtitle="논문·노트·주제·핵심 개념·프로젝트의 관계입니다. 실제 인용(본문에서 확인)과 AI 제안(점선)은 구분해 표시합니다."
        actions={<><Button onClick={suggest}>AI 관련성 제안 만들기</Button><Button kind="ghost" onClick={() => setView({ x: 0, y: 0, k: 1 })}>보기 초기화</Button></>} />
      {!data ? <Spinner label="불러오는 중" /> : data.nodes.length === 0 ? <Empty title="그래프에 표시할 자료가 없습니다">논문을 등록하고 키워드·태그를 정리하거나 노트를 연결하면 관계가 나타납니다.</Empty> : (
        <div className="grid gap-4 lg:grid-cols-[1fr_300px]">
          <div className="overflow-hidden rounded-xl border border-line bg-paper">
            <div className="flex flex-wrap items-center gap-3 border-b border-line px-3 py-2 text-xs">
              <input className={cx(inputBase, 'w-56 py-1')} placeholder="노드 검색" value={q} onChange={(e) => setQ(e.target.value)} />
              {Object.entries(NODE).map(([k, v]) => (
                <label key={k} className="inline-flex items-center gap-1"><input type="checkbox" checked={nodeOn[k]} onChange={(e) => setNodeOn({ ...nodeOn, [k]: e.target.checked })} /><span className="inline-block h-2.5 w-2.5 rounded-full" style={{ background: v.color }} />{v.label}</label>
              ))}
              <div className="flex items-center gap-1"><Button kind="ghost" className="px-2 py-0.5" onClick={() => setView((v) => ({ ...v, k: Math.min(4, v.k * 1.2) }))}>＋</Button><Button kind="ghost" className="px-2 py-0.5" onClick={() => setView((v) => ({ ...v, k: Math.max(0.3, v.k / 1.2) }))}>−</Button></div>
            </div>
            <svg ref={svgRef} viewBox={`0 0 ${W} ${H}`} className="block h-[640px] w-full cursor-grab bg-[#fbf9f3]" onWheel={onWheel}
              onMouseDown={(e) => { if ((e.target as Element).tagName === 'svg') drag.current = { pan: { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y } }; }}
              onMouseMove={(e) => {
                const d = drag.current;
                if (!d) return;
                if (d.node) { const p = toWorld(e); d.node.fx = p.x; d.node.fy = p.y; sim.current?.alphaTarget(0.2).restart(); }
                else if (d.pan) { const r = svgRef.current!.getBoundingClientRect(); setView((v) => ({ ...v, x: d.pan!.vx + (e.clientX - d.pan!.x) * (W / r.width), y: d.pan!.vy + (e.clientY - d.pan!.y) * (H / r.height) })); }
              }}
              onMouseUp={() => { if (drag.current?.node) { drag.current.node.fx = null; drag.current.node.fy = null; sim.current?.alphaTarget(0); } drag.current = null; }}
              onMouseLeave={() => (drag.current = null)}>
              <defs><marker id="arrow" viewBox="0 0 10 10" refX="16" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="#17263D" /></marker></defs>
              <g transform={`translate(${view.x},${view.y}) scale(${view.k})`}>
                {links.map((l) => {
                  const s = l.source as N, t = l.target as N;
                  const st = EDGE[l.kind];
                  const on = sel?.edge?.id === l.id;
                  return <line key={l.id} x1={s.x} y1={s.y} x2={t.x} y2={t.y} stroke={st.color} strokeWidth={on ? st.width + 2 : st.width} strokeDasharray={st.dash} markerEnd={l.kind === 'citation' ? 'url(#arrow)' : undefined} className="cursor-pointer" onClick={() => setSel({ edge: l })} />;
                })}
                {nodes.map((n) => {
                  const st = NODE[n.kind];
                  const hi = match(n) || sel?.node?.id === n.id;
                  return (
                    <g key={n.id} transform={`translate(${n.x ?? 0},${n.y ?? 0})`} className="cursor-pointer" onMouseDown={(e) => { e.stopPropagation(); drag.current = { node: n }; }} onClick={() => setSel({ node: n })} onDoubleClick={() => openNode(n)}>
                      <circle r={st.r + (n.kind === 'concept' ? Math.min(6, n.weight) : 0)} fill={st.color} stroke={hi ? '#C7A05A' : '#fff'} strokeWidth={hi ? 3 : 1.5} />
                      {(view.k > 0.8 || hi || n.kind !== 'concept') && <text x={st.r + 4} y={4} fontSize={11} fill="#1F2430" className="pointer-events-none select-none">{n.label.length > 26 ? `${n.label.slice(0, 26)}…` : n.label}</text>}
                    </g>
                  );
                })}
              </g>
            </svg>
          </div>
          <div className="space-y-4">
            <Card title="연결 종류">
              <div className="space-y-1.5 text-xs">
                {Object.entries(EDGE).map(([k, v]) => (
                  <label key={k} className="flex items-center gap-2"><input type="checkbox" checked={edgeOn[k]} onChange={(e) => setEdgeOn({ ...edgeOn, [k]: e.target.checked })} />
                    <svg width="34" height="8"><line x1="0" y1="4" x2="34" y2="4" stroke={v.color} strokeWidth={v.width} strokeDasharray={v.dash} /></svg>{v.label}</label>
                ))}
              </div>
              <p className="mt-2 text-[11px] text-muted">끌어서 이동, 휠로 확대·축소, 노드 두 번 클릭 시 자료 열기</p>
            </Card>
            {sel?.node && (
              <Card title={NODE[sel.node.kind].label}>
                <p className="text-sm font-semibold text-navy">{sel.node.label}</p>
                <p className="mt-1 text-xs text-muted">연결 {neighbors(sel.node).length}개</p>
                <ul className="mt-2 max-h-48 space-y-1 overflow-y-auto text-xs">
                  {neighbors(sel.node).map((l) => { const o = (l.source as N).id === sel.node!.id ? (l.target as N) : (l.source as N); return <li key={l.id}><Badge tone={l.kind === 'citation' ? 'navy' : l.kind === 'ai_suggested' ? 'gold' : 'muted'}>{EDGE[l.kind].label}</Badge> {o.label}</li>; })}
                </ul>
                {['paper', 'note', 'project'].includes(sel.node.kind) && <Button className="mt-3" kind="primary" onClick={() => openNode(sel.node!)}>열기</Button>}
              </Card>
            )}
            {sel?.edge && (
              <Card title="연결 정보">
                <Badge tone={sel.edge.kind === 'citation' ? 'navy' : sel.edge.kind === 'ai_suggested' ? 'gold' : 'muted'}>{EDGE[sel.edge.kind].label}</Badge>
                <p className="mt-2 text-sm">{(sel.edge.source as N).label} → {(sel.edge.target as N).label}</p>
                <p className="mt-1 text-xs text-muted">{sel.edge.label}{sel.edge.page ? ` · 근거 ${sel.edge.page}쪽` : ''}{sel.edge.score != null ? ` · 유사도 ${sel.edge.score}` : ''}</p>
                {sel.edge.kind === 'ai_suggested' && linkId(sel.edge) && (
                  <div className="mt-3 flex gap-2">
                    <Button kind="primary" onClick={() => api.graphResolve(linkId(sel.edge!)!, true).then(() => { setSel(null); load(); })}>관련 있음 (내 연결로)</Button>
                    <Button onClick={() => api.graphResolve(linkId(sel.edge!)!, false).then(() => { setSel(null); load(); })}>관련 없음</Button>
                  </div>
                )}
                {sel.edge.kind === 'user' && linkId(sel.edge) && <Button className="mt-3" kind="danger" onClick={() => api.graphUnlink(linkId(sel.edge!)!).then(() => { setSel(null); load(); })}>연결 삭제</Button>}
                {sel.edge.kind === 'citation' && sel.edge.page && <Button className="mt-3" onClick={() => go('viewer', { paperId: (sel.edge!.source as N).ref_id, page: sel.edge!.page })}>근거 쪽 열기</Button>}
              </Card>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
