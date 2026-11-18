import { useEffect, useRef, useState } from 'react';
import { ExportButtons, PaperPicker } from '../components/common';
import { api, errMsg, newJob, onJob, type Comparison } from '../lib/api';
import { fmtDate } from '../lib/format';
import { useNav, useOpenSource } from '../lib/nav';
import { Badge, Button, Card, cx, Empty, Field, inputCls, PageHeader, Progress, useToast } from '../lib/ui';

export function ComparisonTable({ c }: { c: Comparison }) {
  const open = useOpenSource();
  return (
    <div className="scroll-thin overflow-x-auto rounded-xl border border-line bg-paper">
      <table className="w-full min-w-[720px] border-collapse text-[13px]">
        <thead>
          <tr>
            <th className="sticky left-0 z-10 w-32 border-b border-line bg-[#efe9dc] px-3 py-2 text-left font-semibold text-navy">비교 항목</th>
            {c.columns.map((col) => (
              <th key={col.paper_id} className="min-w-56 border-b border-l border-line bg-[#efe9dc] px-3 py-2 text-left align-top font-semibold text-navy">
                {col.title}<div className="text-[11px] font-normal text-muted">{col.year ?? ''}</div>
                {col.error && <div className="mt-1 text-[11px] font-normal text-danger">{col.error}</div>}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {c.aspects.map(([key, label]) => (
            <tr key={key}>
              <td className="sticky left-0 z-10 border-b border-line bg-paper px-3 py-2 align-top font-medium text-navy">{label}</td>
              {c.columns.map((col) => {
                const cell = col.cells[key];
                if (!cell || cell.status === 'not_found') return <td key={col.paper_id} className="border-b border-l border-line px-3 py-2 align-top text-muted italic">원문에서 확인되지 않음</td>;
                return (
                  <td key={col.paper_id} className={cx('border-b border-l border-line px-3 py-2 align-top', cell.status === 'weak' && 'bg-gold-soft/40')}>
                    <div>{cell.value}</div>
                    <div className="mt-1.5 flex flex-wrap items-center gap-1">
                      {cell.status === 'weak' && <Badge tone="gold">원문 확인 필요</Badge>}
                      {cell.evidence.map((e, i) => (
                        <button key={i} title={e.quote} onClick={() => open(col.paper_id, e.page, e.quote)} className="rounded bg-navy/8 px-1.5 text-[11px] font-semibold text-navy hover:bg-navy hover:text-white">
                          {e.page ? `p.${e.page}` : '원문'}
                        </button>
                      ))}
                    </div>
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export default function Compare() {
  const { go } = useNav();
  const toast = useToast();
  const [aspects, setAspects] = useState<{ key: string; label: string }[]>([]);
  const [selAspects, setSelAspects] = useState<string[]>([]);
  const [papers, setPapers] = useState<string[]>([]);
  const [title, setTitle] = useState('');
  const [list, setList] = useState<Comparison[]>([]);
  const [cur, setCur] = useState<Comparison | null>(null);
  const [prog, setProg] = useState<{ done: number; total: number; message: string } | null>(null);
  const job = useRef<string | null>(null);
  useEffect(() => {
    api.compareAspects().then((a) => { setAspects(a); setSelAspects(a.map((x) => x.key)); });
    api.compareList().then((l) => { setList(l); });
  }, []);
  const run = async () => {
    const id = newJob();
    job.current = id;
    setProg({ done: 0, total: papers.length, message: '준비 중' });
    const un = await onJob(id, (e) => setProg({ done: e.done, total: e.total, message: e.message }));
    try {
      const c = await api.compareRun(papers, selAspects, title, id);
      setCur(c);
      setList((l) => [c, ...l]);
      toast('비교표를 만들었습니다. 각 칸의 근거 쪽을 원문과 대조해 주세요.', 'ok');
    } catch (e) {
      toast(errMsg(e), 'error');
    } finally {
      un();
      setProg(null);
    }
  };
  return (
    <div>
      <PageHeader title="논문 비교" subtitle="논문 2~10편을 골라 연구 목적·방법·결과·한계 등을 항목별로 비교합니다. 각 칸은 원문 쪽 근거와 연결되며, 원문에서 확인되지 않는 항목은 비워 둡니다." />
      <div className="grid gap-5 lg:grid-cols-[1fr_1fr]">
        <Card title="1. 비교할 논문 선택">
          <PaperPicker selected={papers} onChange={setPapers} max={10} />
        </Card>
        <Card title="2. 비교 항목">
          <div className="grid grid-cols-2 gap-1.5">
            {aspects.map((a) => (
              <label key={a.key} className="flex items-center gap-2 text-sm">
                <input type="checkbox" className="accent-[#7155A4]" checked={selAspects.includes(a.key)} onChange={(e) => setSelAspects(e.target.checked ? [...selAspects, a.key] : selAspects.filter((k) => k !== a.key))} />{a.label}
              </label>
            ))}
          </div>
          <div className="mt-4"><Field label="비교표 제목 (선택)"><input className={inputCls} value={title} onChange={(e) => setTitle(e.target.value)} placeholder="예: 생성형 AI 활용 수업 효과 연구 비교" /></Field></div>
          <div className="mt-4 flex items-center gap-2">
            <Button kind="primary" disabled={papers.length < 2 || selAspects.length === 0 || !!prog} onClick={run}>비교표 만들기</Button>
            {prog && <Button onClick={() => job.current && api.jobCancel(job.current)}>중단</Button>}
            <span className="text-xs text-muted">로컬 AI가 논문마다 관련 문단을 읽고 정리합니다 (논문 수에 따라 몇 분 걸릴 수 있음)</span>
          </div>
          {prog && <div className="mt-3"><Progress done={prog.done} total={prog.total} label={prog.message ? `분석 중: ${prog.message}` : '분석 중'} /></div>}
          <button className="mt-3 text-xs text-violet underline" onClick={() => go('review')}>선택한 논문으로 문헌고찰 개요 만들기 →</button>
        </Card>
      </div>
      {cur && (
        <section className="mt-6">
          <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
            <div><h2 className="text-lg font-semibold text-navy">{cur.title}</h2><p className="text-xs text-muted">{fmtDate(cur.created_at)} · 모델 {cur.model} · AI가 정리한 표입니다. 근거 쪽(p.)을 눌러 원문을 확인하세요.</p></div>
            <ExportButtons target={{ target: 'comparison', id: cur.id }} formats={['xlsx', 'docx', 'md', 'csv']} name={cur.title} />
          </div>
          <ComparisonTable c={cur} />
        </section>
      )}
      <section className="mt-8">
        <h2 className="mb-2 text-base font-semibold text-navy">저장된 비교표</h2>
        {list.length === 0 ? <Empty title="아직 만든 비교표가 없습니다" /> : (
          <ul className="divide-y divide-line/70 rounded-xl border border-line bg-paper">
            {list.map((c) => (
              <li key={c.id} className="flex items-center gap-3 px-4 py-2.5">
                <button className="min-w-0 flex-1 text-left text-sm font-medium hover:text-violet" onClick={() => setCur(c)}>{c.title}</button>
                <span className="text-xs text-muted">{c.columns.length}편 · {fmtDate(c.created_at)}</span>
                <button className="text-xs text-muted hover:text-danger" onClick={() => api.compareDelete(c.id).then(() => { setList((l) => l.filter((x) => x.id !== c.id)); if (cur?.id === c.id) setCur(null); })}>삭제</button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
