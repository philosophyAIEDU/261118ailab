import { ask as confirmDlg } from '@tauri-apps/plugin-dialog';
import { useCallback, useEffect, useState } from 'react';
import { ProviderSection } from '../components/AiProvider';
import { UpdateDialog } from '../components/UpdateDialog';
import { api, errMsg, newJob, onJob, type Settings } from '../lib/api';
import { pickBackup, pickFolder, saveBackup } from '../lib/dialogs';
import { fmtBytes } from '../lib/format';
import { useNav } from '../lib/nav';
import { Badge, Button, Card, cx, Field, inputCls, Modal, PageHeader, Progress, Spinner, Tabs, useToast } from '../lib/ui';

type Tab = 'ai' | 'data' | 'update' | 'security' | 'about';

export default function SettingsPage() {
  const { route, info } = useNav();
  const [tab, setTab] = useState<Tab>(info?.library ? (route.params?.tab ?? 'ai') : 'data');
  return (
    <div>
      <PageHeader title="설정" subtitle="교수님만의 AI 연구실" />
      <Tabs value={tab} onChange={setTab} items={[{ key: 'ai', label: 'AI 모델' }, { key: 'data', label: '연구실 자료·백업' }, { key: 'update', label: '업데이트' }, { key: 'security', label: '보안·연구윤리' }, { key: 'about', label: '프로그램 정보' }]} />
      {tab === 'ai' && <AiTab />}
      {tab === 'data' && <DataTab />}
      {tab === 'update' && <UpdateTab />}
      {tab === 'security' && <SecurityTab />}
      {tab === 'about' && <AboutTab />}
    </div>
  );
}

function AiTab() {
  const toast = useToast();
  const { info } = useNav();
  const [st, setSt] = useState<any>(null);
  const [s, setS] = useState<Settings | null>(null);
  const [pull, setPull] = useState<{ model: string; done: number; total: number; status: string; job: string } | null>(null);
  const [idx, setIdx] = useState<{ done: number; total: number; job: string } | null>(null);
  const load = useCallback(() => api.aiStatus().then((x) => { setSt(x); setS(x.settings); }).catch((e) => toast(errMsg(e), 'error')), [toast]);
  useEffect(() => { if (info?.library) load(); }, [load, info]);
  if (!info?.library) return <p className="text-sm text-muted">먼저 연구실을 열어 주세요.</p>;
  if (!st || !s) return <Spinner label="AI 상태 확인 중" />;
  const save = async (patch: Partial<Settings>) => {
    const n = await api.settingsSave({ ...s, ...patch });
    setS(n);
    toast('저장했습니다', 'ok');
    load();
  };
  const installed: any[] = st.installed.filter((m: any) => !m.is_remote);
  const chats = installed.filter((m) => !m.is_embedding);
  const embeds = installed.filter((m) => m.is_embedding);
  const doPull = async (model: string) => {
    const job = newJob();
    setPull({ model, done: 0, total: 0, status: '시작', job });
    const un = await onJob(job, (e) => setPull({ model, done: e.done, total: e.total, status: e.message, job }));
    try {
      await api.aiPull(model, job);
      toast(`${model} 설치 완료`, 'ok');
      load();
    } catch (e) {
      toast(errMsg(e), 'error');
    } finally {
      un();
      setPull(null);
    }
  };
  const buildIndex = async () => {
    const job = newJob();
    setIdx({ done: 0, total: st.index.pending ?? 0, job });
    const un = await onJob(job, (e) => setIdx({ done: e.done, total: e.total, job }));
    try {
      const n = await api.indexBuild(job);
      toast(`의미 검색 색인을 만들었습니다 (${n}개 문단)`, 'ok');
    } catch (e) {
      toast(errMsg(e), 'error');
    } finally {
      un();
      setIdx(null);
      load();
    }
  };
  const rec = st.recommendation;
  const hw = st.hardware;
  return (
    <div className="space-y-5">
      <ProviderSection settings={s} gemini={st.gemini} onChanged={load} localReady={st.health.running && !!s.chat_model} />
      <h2 className="pt-2 text-lg font-semibold text-navy">로컬 AI (Ollama) — 모델 선택·내려받기</h2>
      <p className="-mt-3 text-sm text-muted">로컬 AI 답변 모델과, 의미 검색에 쓰는 임베딩 모델을 관리합니다. Gemini를 선택해도 의미 검색 색인은 항상 이 컴퓨터의 임베딩 모델로 만듭니다.</p>
      <Card title="로컬 AI 엔진 (Ollama)">
        {st.health.running ? (
          <p className="text-sm">이 컴퓨터에서 Ollama가 실행 중입니다 <Badge tone="teal">v{st.health.version}</Badge> · 주소 127.0.0.1:{s.ollama_port}</p>
        ) : (
          <div className="text-sm">
            <p className="mb-2"><Badge tone="danger">연결 안 됨</Badge> Ollama가 설치되어 있지 않거나 실행 중이 아닙니다. 논문 등록·키워드 검색·PDF 보기는 AI 없이도 사용할 수 있습니다.</p>
            <ol className="mb-3 list-decimal space-y-1 pl-5 text-muted">
              <li>공식 사이트에서 Ollama를 내려받아 설치합니다.</li>
              <li>Ollama를 실행한 뒤 이 화면에서 “다시 확인”을 누릅니다.</li>
              <li>아래 추천 모델을 내려받습니다.</li>
            </ol>
            <div className="flex gap-2"><Button kind="primary" onClick={() => api.openUrl('https://ollama.com/download')}>Ollama 내려받기 (ollama.com)</Button><Button onClick={load}>다시 확인</Button></div>
          </div>
        )}
      </Card>
      <Card title="이 컴퓨터의 사양과 추천">
        <p className="text-sm">{hw.os} · {hw.cpu || hw.arch} · CPU {hw.cores}스레드 · 메모리 {hw.total_ram_gb}GB (사용 가능 {hw.available_ram_gb}GB){hw.apple_silicon ? ' · Apple 칩' : ''}</p>
        <ul className="mt-2 list-disc pl-5 text-xs text-muted">{rec.reasons.map((r: string) => <li key={r}>{r}</li>)}</ul>
        <div className="mt-3 grid gap-2 md:grid-cols-2">
          {st.catalog.map((m: any) => {
            const has = installed.some((i) => i.name === m.name || i.name === `${m.name}:latest`);
            const recommended = m.name === rec.chat || m.name === rec.embed;
            return (
              <div key={m.name} className={cx('rounded-lg border p-3', recommended ? 'border-teal/50 bg-teal-soft/40' : 'border-line bg-white')}>
                <div className="flex items-center gap-1.5"><b className="text-sm text-navy">{m.name}</b><Badge tone={m.role === 'chat' ? 'violet' : 'gold'}>{m.role === 'chat' ? '답변 생성' : '임베딩'}</Badge>{recommended && <Badge tone="teal">추천</Badge>}{has && <Badge tone="navy">설치됨</Badge>}</div>
                <p className="mt-1 text-xs text-muted">{m.label} · 내려받기 약 {m.download_gb}GB · 메모리 약 {m.ram_gb}GB · {m.license}</p>
                <p className="mt-1 text-xs">{m.description}</p>
                {!has && st.health.running && <Button className="mt-2 px-2.5 py-1 text-xs" disabled={!!pull} onClick={() => doPull(m.name)}>내려받기</Button>}
              </div>
            );
          })}
        </div>
        {pull && <div className="mt-3"><Progress done={pull.done} total={pull.total} label={`${pull.model}: ${pull.status} ${pull.total ? `${fmtBytes(pull.done)} / ${fmtBytes(pull.total)}` : ''}`} /><Button className="mt-2 text-xs" onClick={() => api.jobCancel(pull.job)}>중단</Button></div>}
        <p className="mt-3 text-xs text-muted">모델은 Ollama가 공식 저장소(ollama.com)에서 내려받습니다. 내려받을 때만 인터넷을 사용하며, 연구자료는 전송되지 않습니다. 이름에 “cloud”가 들어간 Ollama 클라우드 모델은 연구자료를 외부로 보내므로 이 프로그램에서 사용할 수 없습니다.</p>
      </Card>
      <Card title="역할별 모델 선택">
        <div className="grid gap-3 md:grid-cols-2">
          <Field label="1. 로컬 답변 생성 모델" hint={s.provider === 'gemini' ? '현재 Gemini API로 답변을 생성합니다. 로컬 AI로 전환하면 이 모델을 사용합니다.' : 'AI 검색 답변, 논문 비교, 노트 초안, 문헌고찰'}>
            <select className={inputCls} value={s.chat_model ?? ''} onChange={(e) => save({ chat_model: e.target.value || null })}>
              <option value="">선택 안 함</option>{chats.map((m) => <option key={m.name} value={m.name}>{m.name} {m.parameter_size ? `(${m.parameter_size})` : ''}</option>)}
              {s.chat_model && !chats.some((m) => m.name === s.chat_model) && <option value={s.chat_model}>{s.chat_model} (설치 확인 안 됨)</option>}
            </select>
          </Field>
          <Field label="2. 임베딩 모델 (의미 검색)" hint="바꾸면 의미 검색 색인을 다시 만들어야 합니다">
            <select className={inputCls} value={s.embed_model ?? ''} onChange={(e) => save({ embed_model: e.target.value || null })}>
              <option value="">선택 안 함 (키워드 검색만)</option>{embeds.map((m) => <option key={m.name} value={m.name}>{m.name}</option>)}
              {s.embed_model && !embeds.some((m) => m.name === s.embed_model) && <option value={s.embed_model}>{s.embed_model} (설치 확인 안 됨)</option>}
            </select>
          </Field>
          <Field label="3. 재정렬" hint="현재는 RRF 결합 + 논문당 개수 제한으로 재정렬합니다. 별도 재정렬 모델은 선택 기능으로 향후 지원 예정입니다."><input className={inputCls} disabled value="기본 재정렬 (RRF)" /></Field>
          <Field label="AI에게 줄 근거 문단 수" hint="많을수록 정확하지만 느리고 메모리를 더 씁니다 (3~15)"><input type="number" min={3} max={15} className={inputCls} value={s.top_k} onChange={(e) => setS({ ...s, top_k: Number(e.target.value) })} onBlur={() => save({ top_k: s.top_k })} /></Field>
        </div>
        {chats.length > 0 && rec.chat_light && s.chat_model !== rec.chat_light && <p className="mt-3 text-xs text-muted">메모리가 부족하다는 오류가 나면 더 가벼운 모델 <button className="underline" onClick={() => save({ chat_model: chats.find((m) => m.name.startsWith(rec.chat_light))?.name ?? rec.chat_light })}>{rec.chat_light}</button>로 전환해 보세요.</p>}
      </Card>
      <Card title="의미 검색 색인">
        <p className="text-sm">전체 {st.index.total_chunks.toLocaleString()}개 문단 중 색인이 필요한 문단: <b>{st.index.pending == null ? '임베딩 모델 미선택' : st.index.pending.toLocaleString()}</b></p>
        <div className="mt-3 flex gap-2">
          <Button kind="primary" disabled={!s.embed_model || !st.health.running || !!idx || !st.index.pending} onClick={buildIndex}>의미 검색 색인 만들기</Button>
          {idx && <Button onClick={() => api.jobCancel(idx.job)}>중단</Button>}
        </div>
        {idx && <div className="mt-3"><Progress done={idx.done} total={idx.total} label={`${idx.done} / ${idx.total} 문단`} /></div>}
      </Card>
      <Card title="메모리에 올라간 모델">
        {st.running.length === 0 ? <p className="text-sm text-muted">현재 메모리에 올라간 모델이 없습니다.</p> : (
          <ul className="space-y-1.5">{st.running.map((m: any) => <li key={m.name} className="flex items-center gap-2 text-sm"><b>{m.name}</b><span className="text-xs text-muted">메모리 {fmtBytes(m.size_bytes)}{m.vram_bytes ? ` (그래픽 메모리 ${fmtBytes(m.vram_bytes)})` : ''}</span><Button className="ml-auto px-2.5 py-1 text-xs" onClick={() => api.aiUnload(m.name).then(() => { toast(`${m.name}을(를) 메모리에서 내렸습니다`, 'ok'); load(); }).catch((e) => toast(errMsg(e), 'error'))}>모델 중지</Button></li>)}</ul>
        )}
        {installed.length > 0 && (
          <details className="mt-3 text-sm"><summary className="cursor-pointer text-muted">설치된 모델 관리 ({installed.length}개)</summary>
            <ul className="mt-2 space-y-1">{installed.map((m) => <li key={m.name} className="flex items-center gap-2"><span>{m.name}</span><span className="text-xs text-muted">{fmtBytes(m.size_bytes)}</span><button className="ml-auto text-xs text-muted hover:text-danger" onClick={async () => { if (await confirmDlg(`${m.name} 모델을 이 컴퓨터에서 삭제할까요? (연구자료에는 영향 없음)`, { title: '모델 삭제', kind: 'warning' })) api.aiDeleteModel(m.name).then(load).catch((e) => toast(errMsg(e), 'error')); }}>삭제</button></li>)}</ul>
          </details>
        )}
      </Card>
    </div>
  );
}

function DataTab() {
  const { info, refreshInfo, go } = useNav();
  const toast = useToast();
  const [busy, setBusy] = useState('');
  const [bk, setBk] = useState<{ includeFiles: boolean; pw: string; pw2: string } | null>(null);
  const [rs, setRs] = useState<{ path: string; pw: string; plan?: any; err?: string } | null>(null);
  const [demo, setDemo] = useState<{ done: number; total: number } | null>(null);
  const lib = info?.library;
  const switchTo = async (fn: () => Promise<any>, msg: string) => {
    try {
      await fn();
      toast(msg, 'ok');
      refreshInfo();
      go('dashboard');
    } catch (e) {
      toast(errMsg(e), 'error');
    }
  };
  const doBackup = async () => {
    if (!bk) return;
    if (bk.pw && bk.pw !== bk.pw2) return toast('두 암호가 일치하지 않습니다', 'error');
    if (bk.pw && bk.pw.length < 8) return toast('암호는 8자 이상이어야 합니다', 'error');
    const path = await saveBackup(!!bk.pw);
    if (!path) return;
    setBusy('backup');
    try {
      const r = await api.backupCreate(path, bk.includeFiles, bk.pw || null);
      toast(`백업을 만들고 검증했습니다 (${fmtBytes(r.size)}${r.encrypted ? ', 암호화' : ''})`, 'ok');
      setBk(null);
    } catch (e) {
      toast(errMsg(e), 'error');
    } finally {
      setBusy('');
    }
  };
  const inspect = async () => {
    if (!rs) return;
    setBusy('inspect');
    try {
      const plan = await api.backupInspect(rs.path, rs.pw || null);
      setRs({ ...rs, plan, err: undefined });
    } catch (e) {
      setRs({ ...rs, plan: undefined, err: errMsg(e) });
    } finally {
      setBusy('');
    }
  };
  const restore = async () => {
    if (!rs?.plan) return;
    if (rs.plan.conflict && !(await confirmDlg('현재 연구실의 자료가 백업 내용으로 교체됩니다. 현재 데이터베이스는 복원 직전에 backups 폴더에 안전 사본으로 보관됩니다. 계속할까요?', { title: '복원 확인', kind: 'warning', okLabel: '복원', cancelLabel: '취소' }))) return;
    setBusy('restore');
    try {
      const r = await api.backupRestore(rs.path, rs.pw || null, true);
      toast(`복원했습니다. 파일 ${r.files_restored}개를 되살렸습니다.`, 'ok');
      setRs(null);
      refreshInfo();
      go('dashboard');
    } catch (e) {
      toast(errMsg(e), 'error');
    } finally {
      setBusy('');
    }
  };
  const openDemo = async () => {
    const job = newJob();
    setDemo({ done: 0, total: 1 });
    const un = await onJob(job, (e) => setDemo({ done: e.done, total: e.total }));
    await switchTo(() => api.demoOpen(job), '데모 연구실을 열었습니다. 모든 자료는 가상 자료입니다.');
    un();
    setDemo(null);
  };
  return (
    <div className="space-y-5">
      {info?.open_error && <p className="rounded-lg border border-danger/30 bg-[#fbefed] px-4 py-3 text-sm text-danger">{info.open_error}</p>}
      <Card title="연구실 위치">
        {lib ? <p className="text-sm">현재 연구실: <code className="rounded bg-ivory px-1.5">{lib.root}</code> {lib.demo && <Badge tone="gold">데모 (가상 자료)</Badge>}</p> : <p className="text-sm text-danger">열린 연구실이 없습니다.</p>}
        {lib?.migration_backup && <p className="mt-1 text-xs text-muted">자료 형식을 업그레이드하기 전에 만든 백업: {lib.migration_backup}</p>}
        <p className="mt-2 text-xs text-muted">연구자료(논문 사본·메타데이터·노트·색인·설정)는 프로그램과 분리된 이 폴더에 저장되므로 업데이트·재설치·제거 후에도 유지됩니다. 프로그램을 제거해도 이 폴더는 자동으로 삭제되지 않습니다.</p>
        <div className="mt-3 flex flex-wrap gap-2">
          {lib && <Button onClick={() => api.revealLibrary()}>폴더 열기</Button>}
          <Button onClick={async () => { const d = await pickFolder('연구실로 사용할 폴더 선택 (빈 폴더 또는 기존 연구실 폴더)'); if (d) switchTo(() => api.libraryOpen(d), '연구실을 열었습니다'); }}>다른 연구실 폴더 열기</Button>
          {(lib?.demo || !lib || lib.root !== info.default_root) && <Button onClick={() => switchTo(() => api.libraryOpenDefault(), '기본 연구실을 열었습니다')}>기본 연구실 열기</Button>}
          {!lib?.demo && <Button kind="ghost" disabled={!!demo} onClick={openDemo}>데모 연구실 열기 (가상 자료)</Button>}
        </div>
        {demo && <div className="mt-3"><Progress done={demo.done} total={demo.total} label="데모 연구실 준비 중" /></div>}
        <p className="mt-2 text-xs text-muted">기본 연구실: {info?.default_root} · 데모 연구실: {info?.demo_root}</p>
      </Card>
      {lib && (
        <Card title="백업 및 복원">
          <p className="text-sm">백업에는 논문 메타데이터, 연구 노트, 프로젝트, 비교·문헌고찰 결과, 주석, 검색 색인, AI 모델 설정이 모두 들어갑니다. 사람이 읽을 수 있는 JSON·Markdown 사본도 함께 넣습니다. 백업을 만든 뒤 파일별 해시와 데이터베이스 무결성을 자동으로 검증합니다.</p>
          <div className="mt-3 flex gap-2">
            <Button kind="primary" onClick={() => setBk({ includeFiles: true, pw: '', pw2: '' })}>백업 만들기</Button>
            <Button onClick={async () => { const p = await pickBackup(); if (p) setRs({ path: p, pw: '' }); }}>백업에서 복원…</Button>
          </div>
        </Card>
      )}
      <Modal open={!!bk} onClose={() => setBk(null)} title="연구실 백업" footer={<><Button onClick={() => setBk(null)}>취소</Button><Button kind="primary" disabled={busy === 'backup'} onClick={doBackup}>{busy === 'backup' ? '백업 중…' : '저장 위치 선택 후 백업'}</Button></>}>
        {bk && (
          <div className="space-y-3 text-sm">
            <label className="flex items-start gap-2"><input type="checkbox" className="mt-1" checked={bk.includeFiles} onChange={(e) => setBk({ ...bk, includeFiles: e.target.checked })} /><span><b>PDF 등 원본 사본 포함</b><br /><span className="text-xs text-muted">끄면 용량이 작아지지만, 복원할 때 PDF를 다시 등록해야 합니다.</span></span></label>
            <Field label="암호 (선택, 8자 이상)" hint="민감한 연구자료라면 암호화를 권장합니다. 암호를 잊으면 복구할 수 없습니다."><input type="password" className={inputCls} value={bk.pw} onChange={(e) => setBk({ ...bk, pw: e.target.value })} /></Field>
            {bk.pw && <Field label="암호 확인"><input type="password" className={inputCls} value={bk.pw2} onChange={(e) => setBk({ ...bk, pw2: e.target.value })} /></Field>}
          </div>
        )}
      </Modal>
      <Modal open={!!rs} onClose={() => setRs(null)} title="백업에서 복원" footer={<><Button onClick={() => setRs(null)}>취소</Button>{rs?.plan ? <Button kind="primary" disabled={busy === 'restore'} onClick={restore}>{busy === 'restore' ? '복원 중…' : '복원'}</Button> : <Button kind="primary" disabled={busy === 'inspect'} onClick={inspect}>백업 확인</Button>}</>}>
        {rs && (
          <div className="space-y-3 text-sm">
            <p className="break-all text-xs text-muted">{rs.path}</p>
            <Field label="암호 (암호화된 백업인 경우)"><input type="password" className={inputCls} value={rs.pw} onChange={(e) => setRs({ ...rs, pw: e.target.value, plan: undefined })} /></Field>
            {rs.err && <p className="text-danger">{rs.err}</p>}
            {rs.plan && (
              <div className="rounded-lg bg-ivory p-3">
                <p className="font-semibold text-teal">백업 검증 통과{rs.plan.encrypted ? ' (암호화됨)' : ''}</p>
                <p className="mt-1">만든 날짜: {rs.plan.backup.created_at} · 자료 형식 v{rs.plan.backup.schema_version}</p>
                <p>백업 내용: 논문 {rs.plan.backup.counts.papers}편 · 노트 {rs.plan.backup.counts.notes}개 · 프로젝트 {rs.plan.backup.counts.projects}개{rs.plan.backup.include_files ? ' · 원본 사본 포함' : ' · 원본 사본 미포함'}</p>
                <p>현재 연구실: 논문 {rs.plan.current_counts.papers}편 · 노트 {rs.plan.current_counts.notes}개 · 프로젝트 {rs.plan.current_counts.projects}개</p>
                {rs.plan.conflict && <p className="mt-2 text-[#8a6a2c]">현재 연구실에 자료가 있어 백업 내용으로 교체됩니다. 현재 데이터베이스는 복원 전에 안전 사본으로 보관합니다.</p>}
              </div>
            )}
          </div>
        )}
      </Modal>
    </div>
  );
}

function UpdateTab() {
  const { info } = useNav();
  const toast = useToast();
  const [s, setS] = useState<Settings | null>(null);
  const [res, setRes] = useState<any>(null);
  const [busy, setBusy] = useState(false);
  const [show, setShow] = useState(false);
  useEffect(() => { if (info?.library) api.settings().then(setS); }, [info]);
  const check = async () => {
    setBusy(true);
    try {
      const r = await api.updateCheck(s?.update_channel ?? 'stable');
      setRes(r);
      if (r.available) setShow(true);
    } catch (e) {
      toast(errMsg(e), 'error');
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="space-y-5">
      <Card title="프로그램 업데이트">
        <p className="text-sm">현재 버전: <b>v{info?.version}</b></p>
        {s && (
          <div className="mt-3 grid gap-3 md:grid-cols-2">
            <Field label="업데이트 채널"><select className={inputCls} value={s.update_channel} onChange={(e) => api.settingsSave({ ...s, update_channel: e.target.value as any }).then(setS)}><option value="stable">안정 버전 (권장)</option><option value="beta">시험판 (베타)</option></select></Field>
            <label className="flex items-center gap-2 self-end pb-2 text-sm"><input type="checkbox" checked={s.auto_check_updates} onChange={(e) => api.settingsSave({ ...s, auto_check_updates: e.target.checked }).then(setS)} />프로그램 실행 시 새 버전 확인</label>
          </div>
        )}
        <Button className="mt-3" kind="primary" disabled={busy} onClick={check}>{busy ? '확인 중…' : '지금 업데이트 확인'}</Button>
        {res && !res.available && <p className={cx('mt-3 text-sm', res.configured ? 'text-teal' : 'text-[#8a6a2c]')}>{res.configured ? '최신 버전을 사용하고 있습니다.' : res.message}</p>}
        <p className="mt-3 text-xs text-muted">업데이트 확인 시 GitHub Releases에 현재 버전과 운영체제 정보만 요청합니다. 연구자료는 전송되지 않습니다. 업데이트 파일은 개발자 전자서명을 검증한 뒤에만 설치됩니다.</p>
      </Card>
      {show && res?.available && <UpdateDialog info={res} onClose={() => setShow(false)} />}
    </div>
  );
}

function SecurityTab() {
  return (
    <div className="space-y-5">
      <Card title="데이터 보호 원칙">
        <ul className="list-disc space-y-1.5 pl-5 text-sm">
          <li>연구자료(PDF 사본, 메타데이터, 노트, 검색 색인, 벡터)는 이 컴퓨터의 연구실 폴더에만 저장됩니다.</li>
          <li>AI는 이 컴퓨터에서 실행되는 Ollama(127.0.0.1)만 사용합니다. 시스템 프록시를 거치지 않으며 다른 주소로는 연결하지 않습니다.</li>
          <li><b>클라우드 AI(Gemini API)는 기본으로 꺼져 있습니다.</b> 교수님이 설정에서 직접 선택하고 데이터 전송 안내에 동의한 뒤 API 키를 넣어야만 사용합니다. 이때 질문과 검색된 관련 문단(발췌)만 Google로 전송되며, PDF 전체·노트·의미 검색 색인은 보내지 않습니다. 무료 티어는 Google이 입력 내용을 제품 개선에 사용할 수 있습니다.</li>
          <li>Gemini API 키는 운영체제 보안 저장소(Windows 자격 증명 관리자·macOS 키체인)에만 보관하며 연구실 백업·로그에 넣지 않습니다.</li>
          <li>인터넷을 사용하는 경우: ① AI 모델 내려받기(Ollama가 ollama.com에서), ② 업데이트 확인·설치(GitHub Releases), ③ Gemini API를 선택한 경우의 답변 생성. ①②에서는 연구자료가 전송되지 않습니다.</li>
          <li>원본 파일은 수정하거나 삭제하지 않습니다. 연구실에서 삭제해도 연구실 안의 사본과 기록만 지워집니다.</li>
          <li>PDF 안에 AI에게 지시하는 문장(프롬프트 주입)이 있어도 연구자료로만 취급하도록 처리하고, 의심 문장이 있는 출처는 “주의” 표시를 합니다.</li>
          <li>오류 메시지와 기록에는 연구자료 원문을 넣지 않습니다.</li>
        </ul>
        <p className="mt-3 text-xs text-muted">로컬 AI를 사용한다고 해서 완전한 보안이 자동으로 보장되는 것은 아닙니다. 컴퓨터 자체의 보안(운영체제 업데이트, 디스크 암호화, 계정 암호)과 백업 파일 관리도 함께 신경 써 주세요.</p>
      </Card>
      <Card title="연구윤리 안내">
        <ul className="list-disc space-y-1.5 pl-5 text-sm">
          <li>AI가 생성한 답변·비교표·노트 초안·문헌고찰은 검토용 초안입니다. 반드시 원문과 대조해 확인해 주세요.</li>
          <li>이 프로그램은 실제로 검색된 문서의 출처만 표시하며, 검증되지 않은 출처 표기와 등록 자료에 없는 인용은 제거합니다. 그래도 요약 내용의 정확성은 교수님의 확인이 필요합니다.</li>
          <li>학술지와 소속 기관의 생성형 AI 활용 규정을 확인하고, 필요한 경우 AI 사용 사실을 밝혀 주세요.</li>
          <li>저작권이 있는 논문 PDF는 개인 연구 목적 범위에서 사용해 주세요.</li>
        </ul>
      </Card>
    </div>
  );
}

function AboutTab() {
  const { info } = useNav();
  return (
    <Card>
      <div className="flex items-center gap-4">
        <img src="/icon.png" alt="" className="h-16 w-16" />
        <div>
          <h2 className="text-xl font-bold text-navy">교수님만의 AI 연구실</h2>
          <p className="text-sm text-muted">Philosophy Research Hub · v{info?.version}</p>
        </div>
      </div>
      <p className="mt-4 text-[15px]">“교수님의 논문과 연구자료를 모으고, 검색하고, 비교하고, 연결하세요.”</p>
      <p className="mt-1 text-sm text-muted">흩어진 연구자료를 평생 활용할 수 있는 나만의 지식 자산으로 만듭니다.</p>
      <dl className="mt-4 grid grid-cols-[140px_1fr] gap-y-1 text-sm">
        <dt className="text-muted">개발 및 제공</dt><dd>필로소피 AI 교육</dd>
        <dt className="text-muted">브랜드 철학</dt><dd>AI로 성장하고, 그 성장을 나눈다.</dd>
        <dt className="text-muted">운영체제</dt><dd>{info?.platform} ({info?.arch})</dd>
        <dt className="text-muted">자료 형식 버전</dt><dd>v{info?.schema_version}</dd>
        <dt className="text-muted">설정 폴더</dt><dd className="break-all">{info?.config_dir}</dd>
        <dt className="text-muted">라이선스</dt><dd>MIT</dd>
      </dl>
      <Button className="mt-4" onClick={() => api.openUrl('https://github.com/philosophyAIEDU/261118ailab')}>GitHub 저장소</Button>
    </Card>
  );
}
