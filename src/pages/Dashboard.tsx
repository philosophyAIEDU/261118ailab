import { useEffect, useState } from 'react';
import { api, errMsg, newJob, onJob, type Paper } from '../lib/api';
import { authorsShort, fmtDate } from '../lib/format';
import { useNav } from '../lib/nav';
import { Badge, Button, Card, Empty, Progress, Spinner, useToast } from '../lib/ui';

function Stat({ label, value, sub, onClick }: { label: string; value: number | string; sub?: string; onClick?: () => void }) {
  return (
    <button onClick={onClick} className="rounded-xl border border-line bg-paper p-4 text-left hover:border-navy/30">
      <div className="text-[12.5px] text-muted">{label}</div>
      <div className="mt-1 text-[26px] leading-none font-bold text-navy">{value}</div>
      {sub && <div className="mt-1.5 text-xs text-muted">{sub}</div>}
    </button>
  );
}

export default function Dashboard() {
  const { go, refreshInfo } = useNav();
  const toast = useToast();
  const [d, setD] = useState<any>(null);
  const [ai, setAi] = useState<any>(null);
  const [err, setErr] = useState('');
  const [demo, setDemo] = useState<{ done: number; total: number } | null>(null);
  useEffect(() => {
    api.dashboard().then(setD).catch((e) => setErr(errMsg(e)));
    api.aiStatus().then(setAi).catch(() => setAi({ health: { running: false } }));
  }, []);
  if (err) return <Empty title="연구실을 불러오지 못했습니다">{err}</Empty>;
  if (!d) return <Spinner label="불러오는 중" />;
  const s = d.stats;
  const openDemo = async () => {
    const job = newJob();
    setDemo({ done: 0, total: 1 });
    const un = await onJob(job, (e) => setDemo({ done: e.done, total: e.total }));
    try {
      await api.demoOpen(job);
      toast('데모 연구실을 열었습니다. 모든 자료는 가상 자료입니다.', 'ok');
      refreshInfo();
      go('dashboard');
      setD(await api.dashboard());
    } catch (e) {
      toast(errMsg(e), 'error');
    } finally {
      un();
      setDemo(null);
    }
  };
  const chat = d.settings.chat_model;
  const embed = d.settings.embed_model;
  return (
    <div>
      <header className="mb-6">
        <h1 className="text-[26px] font-bold tracking-tight text-navy">교수님만의 AI 연구실</h1>
        <p className="mt-0.5 text-[15px] text-muted">논문을 모으고, 연구의 연결을 발견하세요.</p>
      </header>
      {d.demo && <div className="mb-5 rounded-lg border border-gold/40 bg-gold-soft px-4 py-2.5 text-sm text-[#7a5b22]">지금은 <b>데모 연구실</b>입니다. 등록된 논문·저자·수치는 모두 시연용 가상 자료이며 실제 논문이 아닙니다. 설정 &gt; 연구실 자료에서 내 연구실로 돌아갈 수 있습니다.</div>}

      {s.papers === 0 ? (
        <Card className="mb-6">
          <h2 className="text-lg font-semibold text-navy">연구실이 비어 있습니다</h2>
          <p className="mt-1 text-sm text-muted">PDF 논문을 등록하면 제목·키워드·초록을 추출하고, 쪽 번호와 함께 검색할 수 있게 정리합니다. 원본 파일은 수정하지 않습니다.</p>
          <div className="mt-4 flex flex-wrap gap-2">
            <Button kind="primary" onClick={() => go('papers', { import: true })}>첫 논문 등록하기</Button>
            <Button onClick={() => go('zotero')}>Zotero 서지 가져오기</Button>
            {!d.demo && <Button kind="ghost" onClick={openDemo} disabled={!!demo}>데모 연구실 열어 보기 (가상 자료)</Button>}
          </div>
          {demo && <div className="mt-4"><Progress done={demo.done} total={demo.total} label="데모 연구실 준비 중" /></div>}
        </Card>
      ) : null}

      <div className="mb-6 grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Stat label="저장된 논문" value={s.papers} sub={s.papers ? `본문 검색 가능 ${s.papers_with_text}편${s.needs_attention ? ` · 확인 필요 ${s.needs_attention}편` : ''}` : '아직 없음'} onClick={() => go('papers')} />
        <Stat label="연구 노트" value={s.notes} sub={s.notes ? undefined : '아직 없음'} onClick={() => go('notes')} />
        <Stat label="진행 중인 프로젝트" value={s.projects_active} sub={s.projects_active ? undefined : '아직 없음'} onClick={() => go('projects')} />
        <Stat label="의미 검색 색인" value={s.chunks ? `${Math.round((s.embedded_chunks / s.chunks) * 100)}%` : '–'} sub={s.chunks ? `${s.embedded_chunks.toLocaleString()} / ${s.chunks.toLocaleString()} 문단` : '등록된 본문 없음'} onClick={() => go('settings', { tab: 'ai' })} />
      </div>

      <div className="grid gap-5 lg:grid-cols-[1.4fr_1fr]">
        <Card title="최근 등록한 논문" action={<Button kind="ghost" className="text-xs" onClick={() => go('papers')}>전체 보기 →</Button>}>
          {d.recent_papers.length === 0 ? <p className="text-sm text-muted">등록한 논문이 없습니다.</p> : (
            <ul className="divide-y divide-line/70">
              {d.recent_papers.map((p: Paper) => (
                <li key={p.id} className="py-2">
                  <button className="text-left text-sm font-medium text-ink hover:text-violet" onClick={() => go('papers', { id: p.id })}>{p.title}</button>
                  <div className="text-xs text-muted">{authorsShort(p.authors)} · {p.year ?? '연도 미상'} · 등록 {fmtDate(p.created_at)}</div>
                </li>
              ))}
            </ul>
          )}
        </Card>
        <div className="flex flex-col gap-5">
          <Card title="현재 AI 모델 상태" action={<Button kind="ghost" className="text-xs" onClick={() => go('settings', { tab: 'ai' })}>관리 →</Button>}>
            {!ai ? <Spinner label="확인 중" /> : (
              <div className="space-y-1.5 text-sm">
                <div className="flex items-center gap-2">로컬 AI(Ollama) {ai.health?.running ? <Badge tone="teal">실행 중 {ai.health.version ? `v${ai.health.version}` : ''}</Badge> : <Badge tone="danger">연결 안 됨</Badge>}</div>
                <div>답변 생성: {d.settings.provider === 'gemini' ? <><b className="text-navy">{d.settings.gemini_model}</b> <Badge tone="violet">Gemini API · 클라우드</Badge></> : chat ? <b className="text-navy">{chat} (로컬)</b> : <span className="text-muted">선택 안 됨</span>}</div>
                <div>임베딩 모델: {embed ? <b className="text-navy">{embed}</b> : <span className="text-muted">선택 안 됨</span>}</div>
                {ai.running?.length > 0 && <div className="text-xs text-muted">메모리에 올라간 모델: {ai.running.map((r: any) => r.name).join(', ')}</div>}
                <div className="pt-1 text-xs text-muted">{d.settings.provider === 'gemini' ? 'Gemini 사용 중: 질문과 관련 문단이 Google로 전송됩니다. 설정에서 로컬 AI로 되돌릴 수 있습니다.' : '클라우드 AI 사용 안 함 — 연구자료는 외부로 전송되지 않습니다.'}</div>
              </div>
            )}
          </Card>
          <Card title="최근 AI 검색" action={<Button kind="ghost" className="text-xs" onClick={() => go('search')}>검색하기 →</Button>}>
            {d.recent_searches.length === 0 ? <p className="text-sm text-muted">아직 검색 기록이 없습니다.</p> : (
              <ul className="space-y-1.5">
                {d.recent_searches.map((r: any) => (
                  <li key={r.id}><button className="text-left text-sm hover:text-violet" onClick={() => go('search', { resultId: r.id })}>{r.question}</button> <span className="text-xs text-muted">{fmtDate(r.created_at)}</span></li>
                ))}
              </ul>
            )}
          </Card>
          <Card title="진행 중인 프로젝트">
            {d.projects.length === 0 ? <p className="text-sm text-muted">진행 중인 프로젝트가 없습니다.</p> : (
              <ul className="space-y-1">{d.projects.map((p: any) => <li key={p.id}><button className="text-sm hover:text-violet" onClick={() => go('projects', { id: p.id })}>{p.name}</button> <span className="text-xs text-muted">논문 {p.paper_ids.length}편 · 노트 {p.note_count}개</span></li>)}</ul>
            )}
          </Card>
        </div>
      </div>
    </div>
  );
}
