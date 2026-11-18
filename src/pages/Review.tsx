import { useEffect, useRef, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { CloudBanner } from '../components/AiProvider';
import { ExportButtons, PaperPicker } from '../components/common';
import { api, errMsg, newJob, onJob, type Review as R, type Settings, type TodayUsage } from '../lib/api';
import { fmtDate } from '../lib/format';
import { useOpenSource } from '../lib/nav';
import { Badge, Button, Card, Empty, Field, inputCls, PageHeader, useToast } from '../lib/ui';

export default function Review() {
  const toast = useToast();
  const open = useOpenSource();
  const [papers, setPapers] = useState<string[]>([]);
  const [topic, setTopic] = useState('');
  const [busy, setBusy] = useState(false);
  const [stream, setStream] = useState('');
  const [list, setList] = useState<R[]>([]);
  const [cur, setCur] = useState<R | null>(null);
  const job = useRef<string | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [usage, setUsage] = useState<TodayUsage | null>(null);
  useEffect(() => { api.reviewList().then(setList); api.settings().then(setSettings).catch(() => {}); api.geminiUsage().then(setUsage).catch(() => {}); }, []);
  const run = async () => {
    const id = newJob();
    job.current = id;
    setBusy(true);
    setStream('');
    const un = await onJob(id, (e) => { if (e.stage === 'token') setStream((s) => s + e.message); if (e.stage === 'notice') setStream((s) => s + `\n(${e.message})\n`); });
    try {
      const r = await api.reviewRun(papers, topic, id);
      setCur(r);
      setList((l) => [r, ...l]);
    } catch (e) {
      toast(errMsg(e), 'error');
    } finally {
      un();
      setBusy(false);
    }
  };
  const byId = (pid: string) => cur?.citations.find((c) => c.paper_id === pid)?.paper_title;
  return (
    <div>
      <PageHeader title="문헌고찰 지원" subtitle="선택한 논문만으로 주제별 분류, 연구 흐름·방법론·결과·한계 비교, 연구 공백 후보, 문헌고찰 개요 초안을 만듭니다. 결과는 검토용 초안입니다." />
      <CloudBanner settings={settings} usage={usage} />
      <div className="grid gap-5 lg:grid-cols-[1fr_1fr]">
        <Card title="논문 선택 (2~15편)"><PaperPicker selected={papers} onChange={setPapers} max={15} /></Card>
        <Card title="문헌고찰 주제">
          <Field label="주제" hint="예: 생성형 AI 활용 수업이 대학생의 비판적 사고에 미치는 영향"><input className={inputCls} value={topic} onChange={(e) => setTopic(e.target.value)} /></Field>
          <div className="mt-4 flex gap-2">
            <Button kind="primary" disabled={papers.length < 2 || busy} onClick={run}>개요 초안 만들기</Button>
            {busy && <Button onClick={() => job.current && api.jobCancel(job.current)}>중단</Button>}
          </div>
          <ul className="mt-4 list-disc space-y-1 pl-5 text-xs text-muted">
            <li>주제별 분류는 공유 키워드·태그·연구 분야에 따른 규칙 기반 분류입니다 (AI 사용 안 함).</li>
            <li>참고문헌 목록은 AI가 만들지 않고, 등록된 서지정보로만 만듭니다.</li>
            <li>연구 공백은 선택한 자료 범위에서 도출한 후보일 뿐입니다.</li>
            <li>학술지·소속 기관의 생성형 AI 활용 규정을 확인하고, 사용 사실을 적절히 밝혀 주세요.</li>
          </ul>
          {busy && <div className="mt-4 rounded-lg bg-ivory p-3 text-xs whitespace-pre-wrap text-muted"><Badge tone="gold">작성 중 — 검증 전</Badge>{'\n'}{stream || '관련 문단을 모으는 중…'}</div>}
        </Card>
      </div>
      {cur && (
        <section className="mt-6 space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h2 className="text-lg font-semibold text-navy">{cur.title}</h2>
            <ExportButtons target={{ target: 'review', id: cur.id }} formats={['docx', 'md', 'txt']} name={cur.title} />
          </div>
          <p className="rounded-lg border border-gold/40 bg-gold-soft px-4 py-2.5 text-sm text-[#7a5b22]">{cur.notice}</p>
          <Card title="연구 주제별 분류 (규칙 기반)">
            <ul className="space-y-1 text-sm">{cur.groups.map((g) => <li key={g.label}><Badge tone={g.basis === 'none' ? 'muted' : 'violet'}>{g.label}</Badge> <span className="text-muted">{g.paper_ids.length}편</span></li>)}</ul>
          </Card>
          <Card title={<span className="flex items-center gap-2">개요 초안 <Badge tone="violet">AI 생성 · 검토 필요</Badge></span>}>
            <div className="prose-lab"><ReactMarkdown remarkPlugins={[remarkGfm]}>{cur.draft}</ReactMarkdown></div>
            {(cur.removed_markers > 0 || cur.removed_references.length > 0) && <p className="mt-3 text-xs text-muted">인용 검증: 존재하지 않는 출처 표기 {cur.removed_markers}개, 확인되지 않은 인용 {cur.removed_references.length}개를 제거했습니다.</p>}
          </Card>
          <Card title="근거 출처">
            <ul className="space-y-1.5 text-sm">
              {cur.citations.map((c) => (
                <li key={c.label} className="flex items-start gap-2">
                  <Badge tone="navy">{c.label}</Badge>
                  <button className="text-left hover:text-violet" onClick={() => open(c.paper_id, c.page, c.quote)}>{byId(c.paper_id)} {c.page ? `(${c.page}쪽)` : ''}</button>
                  {c.support === 'verified' ? <Badge tone="teal">근거 확인</Badge> : <Badge tone="gold">원문 확인 필요</Badge>}
                </li>
              ))}
            </ul>
          </Card>
          <Card title="참고문헌 (등록된 서지정보 기준)">
            <ul className="space-y-1 text-sm">{cur.references.map((r) => <li key={r} className="pl-4 -indent-4">{r}</li>)}</ul>
          </Card>
        </section>
      )}
      <section className="mt-8">
        <h2 className="mb-2 text-base font-semibold text-navy">저장된 문헌고찰 초안</h2>
        {list.length === 0 ? <Empty title="아직 없습니다" /> : (
          <ul className="divide-y divide-line/70 rounded-xl border border-line bg-paper">
            {list.map((r) => (
              <li key={r.id} className="flex items-center gap-3 px-4 py-2.5">
                <button className="min-w-0 flex-1 text-left text-sm font-medium hover:text-violet" onClick={() => setCur(r)}>{r.title}</button>
                <span className="text-xs text-muted">{r.paper_ids.length}편 · {fmtDate(r.created_at)}</span>
                <button className="text-xs text-muted hover:text-danger" onClick={() => api.reviewDelete(r.id).then(() => { setList((l) => l.filter((x) => x.id !== r.id)); if (cur?.id === r.id) setCur(null); })}>삭제</button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
