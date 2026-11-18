import { useCallback, useEffect, useState } from 'react';
import { api, errMsg, type Settings, type TodayUsage } from '../lib/api';
import { Badge, Button, Card, cx, inputCls, Modal, useToast } from '../lib/ui';

const GEMINI_LABEL: Record<string, string> = {
  'gemini-3.5-flash-lite': 'Gemini 3.5 Flash-Lite',
  'gemini-3.8-flash': 'Gemini 3.8 Flash',
};
export const geminiName = (m: string) => GEMINI_LABEL[m] ?? m;

/** 한국 시간으로 초기화 시각 표시 */
function resetText(iso: string) {
  try {
    const d = new Date(iso);
    return d.toLocaleString('ko-KR', { timeZone: 'Asia/Seoul', month: 'long', day: 'numeric', hour: 'numeric', minute: '2-digit' });
  } catch {
    return iso;
  }
}

/** 오늘의 Gemini 무료 사용량 (모델별) */
export function UsageCard({ usage, settings, onLimits, compact }: { usage: TodayUsage; settings: Settings; onLimits?: (model: string, rpm: number, rpd: number) => void; compact?: boolean }) {
  const [edit, setEdit] = useState<string | null>(null);
  const [vals, setVals] = useState({ rpm: 0, rpd: 0 });
  const rows = compact ? usage.models.filter((m) => m.model === settings.gemini_model) : usage.models;
  return (
    <div>
      <div className="space-y-3">
        {rows.map((m) => {
          const pct = m.limits.rpd > 0 ? Math.min(100, Math.round((m.requests / m.limits.rpd) * 100)) : 0;
          const active = m.model === settings.gemini_model && settings.provider === 'gemini';
          return (
            <div key={m.model} className={cx(!compact && 'rounded-lg border border-line bg-white p-3', active && !compact && 'border-violet/40')}>
              <div className="mb-1 flex flex-wrap items-center gap-1.5 text-sm">
                <b className="text-navy">{geminiName(m.model)}</b>
                {active && <Badge tone="violet">사용 중</Badge>}
                <span className="ml-auto text-xs text-muted">분당 {m.limits.rpm}회 · 하루 {m.limits.rpd.toLocaleString()}회</span>
              </div>
              <div className="flex items-center gap-2">
                <div className="h-2.5 flex-1 overflow-hidden rounded-full bg-[#ebe5d8]">
                  <div className={cx('h-full', pct >= 90 ? 'bg-danger' : pct >= 70 ? 'bg-gold' : 'bg-teal')} style={{ width: `${pct}%` }} />
                </div>
                <span className="w-36 text-right text-sm"><b>{m.requests.toLocaleString()}</b> / {m.limits.rpd.toLocaleString()}회 <span className="text-xs text-muted">(남음 {m.remaining.toLocaleString()})</span></span>
              </div>
              {!compact && (
                <div className="mt-1 flex flex-wrap items-center gap-x-3 text-xs text-muted">
                  <span>입력 토큰 {m.input_tokens.toLocaleString()} · 출력 토큰 {m.output_tokens.toLocaleString()}</span>
                  {m.errors > 0 && <span>오류 {m.errors}회</span>}
                  {onLimits && edit !== m.model && <button className="ml-auto underline" onClick={() => { setEdit(m.model); setVals({ rpm: m.limits.rpm, rpd: m.limits.rpd }); }}>한도 수정</button>}
                </div>
              )}
              {onLimits && edit === m.model && (
                <div className="mt-2 flex flex-wrap items-center gap-2 text-xs">
                  분당 <input type="number" min={1} className="w-16 rounded border border-line px-1.5 py-1" value={vals.rpm} onChange={(e) => setVals({ ...vals, rpm: Number(e.target.value) })} />
                  하루 <input type="number" min={1} className="w-20 rounded border border-line px-1.5 py-1" value={vals.rpd} onChange={(e) => setVals({ ...vals, rpd: Number(e.target.value) })} />
                  <Button className="px-2 py-1 text-xs" kind="primary" onClick={() => { onLimits(m.model, Math.max(1, vals.rpm), Math.max(1, vals.rpd)); setEdit(null); }}>저장</Button>
                  <Button className="px-2 py-1 text-xs" kind="ghost" onClick={() => setEdit(null)}>취소</Button>
                </div>
              )}
            </div>
          );
        })}
      </div>
      <p className="mt-2 text-xs text-muted">
        오늘 사용량은 <b>이 컴퓨터에서 보낸 요청</b>을 센 추정치입니다. 같은 키를 다른 곳에서도 쓰면 실제 사용량은 더 많습니다. 무료 한도는 태평양 시간 자정, 한국 시간 <b>{resetText(usage.reset_at)}</b>에 초기화됩니다.
      </p>
    </div>
  );
}

/** Gemini 사용 전 데이터 전송·비용 안내 */
export function ConsentModal({ open, onClose, onAccept }: { open: boolean; onClose: () => void; onAccept: () => void }) {
  const [ok, setOk] = useState(false);
  useEffect(() => { if (open) setOk(false); }, [open]);
  return (
    <Modal open={open} onClose={onClose} title="Gemini API 사용 전에 확인해 주세요" wide
      footer={<><Button onClick={onClose}>취소 (로컬 AI 유지)</Button><Button kind="primary" disabled={!ok} onClick={onAccept}>동의하고 Gemini 사용</Button></>}>
      <div className="space-y-4 text-sm">
        <p>Gemini API는 Google 서버에서 실행되는 <b>클라우드 AI</b>입니다. 로컬 AI와 달리 일부 연구자료가 인터넷을 통해 Google로 전송됩니다.</p>
        <div className="rounded-lg border border-line bg-white p-3">
          <h4 className="mb-1 font-semibold text-navy">Google로 전송되는 것</h4>
          <ul className="list-disc space-y-0.5 pl-5">
            <li>AI 연구 검색: 질문, 검색된 관련 문단(기본 8개 이내), 해당 논문의 제목·연도·쪽 번호</li>
            <li>논문 비교·연구 노트 초안: 선택한 논문에서 뽑은 발췌 문단(논문당 최대 12개)</li>
            <li>문헌고찰: 선택한 논문의 제목·연도와 발췌 문단</li>
          </ul>
          <h4 className="mt-3 mb-1 font-semibold text-navy">전송하지 않는 것</h4>
          <ul className="list-disc space-y-0.5 pl-5">
            <li>PDF 파일 전체, 연구 노트, 메모·주석, 연구실 전체 목록</li>
            <li>의미 검색 색인(임베딩)은 계속 이 컴퓨터의 로컬 AI로만 만듭니다</li>
          </ul>
        </div>
        <div className="rounded-lg border border-gold/40 bg-gold-soft p-3 text-[#6f5320]">
          <h4 className="mb-1 font-semibold">무료 티어의 데이터 이용</h4>
          <p>Google의 Gemini API 추가 이용약관에 따르면 <b>무료(미결제) 사용 시 보낸 내용과 응답이 Google 제품 개선에 사용될 수 있고, 사람 검토자가 볼 수 있습니다.</b> 미발표 원고, 개인정보(학생 정보 등), 보안이 필요한 연구자료가 담긴 질문에는 로컬 AI를 사용해 주세요.</p>
        </div>
        <div className="rounded-lg border border-line bg-white p-3">
          <h4 className="mb-1 font-semibold text-navy">비용과 한도</h4>
          <ul className="list-disc space-y-0.5 pl-5">
            <li>무료 티어 키는 요금이 청구되지 않습니다. 하루·분당 사용 한도를 넘으면 다음 초기화까지 오류가 납니다.</li>
            <li>결제 수단을 연결한 Google Cloud 프로젝트의 키를 넣으면 사용량에 따라 <b>요금이 청구될 수 있습니다.</b></li>
            <li>API 키는 이 컴퓨터의 운영체제 보안 저장소에만 보관하며, 연구실 백업에는 포함되지 않습니다.</li>
          </ul>
        </div>
        <p className="text-xs text-muted">언제든 설정에서 로컬 AI로 되돌리거나 키를 삭제할 수 있습니다. 학술지·소속 기관의 생성형 AI·외부 서비스 이용 규정도 함께 확인해 주세요.</p>
        <label className="flex items-center gap-2 font-medium"><input type="checkbox" className="accent-[#7155A4]" checked={ok} onChange={(e) => setOk(e.target.checked)} />위 내용을 확인했으며, 선택한 자료가 Google로 전송되는 것에 동의합니다.</label>
      </div>
    </Modal>
  );
}

/** 설정 > AI 모델 상단: 답변 생성 AI 선택 + Gemini 설정 */
export function ProviderSection({ settings, gemini, onChanged, localReady }: { settings: Settings; gemini: any; onChanged: () => void; localReady: boolean }) {
  const toast = useToast();
  const [consentOpen, setConsentOpen] = useState(false);
  const [keyInput, setKeyInput] = useState('');
  const [busy, setBusy] = useState(false);
  const [avail, setAvail] = useState<{ id: string; available: boolean }[] | null>(null);
  const [usage, setUsage] = useState<TodayUsage>(gemini.usage);
  const [view, setView] = useState<'ollama' | 'gemini'>(settings.provider);
  useEffect(() => setUsage(gemini.usage), [gemini.usage]);
  const save = useCallback(async (patch: Partial<Settings>) => {
    try {
      await api.settingsSave({ ...settings, ...patch });
      onChanged();
    } catch (e) {
      toast(errMsg(e), 'error');
    }
  }, [settings, onChanged, toast]);

  const chooseGemini = () => {
    setView('gemini');
    if (!settings.gemini_consent_at) setConsentOpen(true);
    else if (gemini.key_stored) save({ provider: 'gemini' });
  };
  const chooseLocal = () => {
    setView('ollama');
    if (settings.provider !== 'ollama') save({ provider: 'ollama' }).then(() => toast('로컬 AI로 전환했습니다. 연구자료가 외부로 전송되지 않습니다.', 'ok'));
  };
  const accept = async () => {
    setConsentOpen(false);
    try {
      await api.geminiConsent(true);
      onChanged();
      toast(gemini.key_stored ? '동의했습니다. Gemini 사용으로 전환하려면 아래에서 “Gemini로 답변 생성”을 누르세요.' : '동의했습니다. 이제 아래에 Gemini API 키를 입력해 주세요.', 'ok');
    } catch (e) {
      toast(errMsg(e), 'error');
    }
  };
  const saveKey = async () => {
    setBusy(true);
    try {
      const r = await api.geminiKeySet(keyInput);
      setAvail(r.models);
      setKeyInput('');
      toast(`키를 확인하고 안전하게 저장했습니다 (${r.masked})`, 'ok');
      await api.settingsSave({ ...settings, provider: 'gemini' });
      onChanged();
    } catch (e) {
      toast(errMsg(e), 'error');
    } finally {
      setBusy(false);
    }
  };
  const verify = async () => {
    setBusy(true);
    try {
      const r = await api.geminiKeyVerify();
      setAvail(r.models);
      toast('키가 정상입니다', 'ok');
    } catch (e) {
      toast(errMsg(e), 'error');
    } finally {
      setBusy(false);
    }
  };
  const del = async () => {
    await api.geminiKeyDelete();
    setAvail(null);
    toast('키를 삭제하고 로컬 AI로 되돌렸습니다', 'ok');
    onChanged();
  };
  const setLimits = async (model: string, rpm: number, rpd: number) => {
    await save({ gemini_limits: { ...settings.gemini_limits, [model]: { rpm, rpd } } });
    setUsage(await api.geminiUsage());
  };
  const usingGemini = settings.provider === 'gemini';

  return (
    <Card title="답변 생성 AI 선택">
      <div className="grid gap-3 md:grid-cols-2" role="radiogroup" aria-label="답변 생성 AI">
        <button role="radio" aria-checked={!usingGemini} onClick={chooseLocal} className={cx('rounded-xl border p-4 text-left', !usingGemini ? 'border-teal bg-teal-soft/50' : 'border-line bg-white hover:border-navy/30')}>
          <div className="flex items-center gap-2"><b className="text-navy">로컬 AI (Ollama)</b>{!usingGemini && <Badge tone="teal">사용 중</Badge>}<Badge>권장</Badge></div>
          <ul className="mt-1.5 list-disc pl-5 text-xs text-[#3b3f48]">
            <li>연구자료가 이 컴퓨터 밖으로 나가지 않음</li>
            <li>무료·사용량 제한 없음, 인터넷 없이 동작</li>
            <li>컴퓨터 사양에 따라 속도·품질이 달라짐 {localReady ? '' : '(아래에서 모델 설치 필요)'}</li>
          </ul>
        </button>
        <button role="radio" aria-checked={usingGemini} onClick={chooseGemini} className={cx('rounded-xl border p-4 text-left', usingGemini ? 'border-violet bg-violet-soft/60' : 'border-line bg-white hover:border-navy/30')}>
          <div className="flex items-center gap-2"><b className="text-navy">Gemini API (Google 클라우드)</b>{usingGemini && <Badge tone="violet">사용 중</Badge>}<Badge tone="gold">무료 티어</Badge></div>
          <ul className="mt-1.5 list-disc pl-5 text-xs text-[#3b3f48]">
            <li>컴퓨터 사양과 관계없이 빠르고 정확한 답변</li>
            <li>질문과 관련 문단이 Google로 전송됨 (동의 필요)</li>
            <li>무료 티어는 하루·분당 사용 한도가 있음</li>
          </ul>
        </button>
      </div>

      {view === 'gemini' && (
        <div className="mt-5 space-y-4 border-t border-line pt-4">
          {!settings.gemini_consent_at ? (
            <div className="rounded-lg bg-gold-soft px-4 py-3 text-sm text-[#6f5320]">Gemini를 사용하려면 먼저 데이터 전송 안내를 확인해 주세요. <Button className="ml-2 px-2.5 py-1 text-xs" kind="primary" onClick={() => setConsentOpen(true)}>안내 보기</Button></div>
          ) : (
            <>
              <div>
                <h4 className="mb-1 text-sm font-semibold text-navy">1. Gemini API 키</h4>
                {gemini.key_stored ? (
                  <div className="flex flex-wrap items-center gap-2 text-sm">
                    <span>저장된 키: <code className="rounded bg-ivory px-1.5">{gemini.key_masked}</code></span>
                    <Badge tone="teal">{gemini.key_backend === 'os_keychain' ? '운영체제 보안 저장소에 보관' : '설정 폴더에 보관 (개발용)'}</Badge>
                    <Button className="px-2.5 py-1 text-xs" disabled={busy} onClick={verify}>키 확인</Button>
                    <Button kind="danger" className="px-2.5 py-1 text-xs" onClick={del}>키 삭제</Button>
                  </div>
                ) : (
                  <p className="mb-2 text-xs text-muted">
                    <button className="underline" onClick={() => api.openUrl('https://aistudio.google.com/apikey')}>Google AI Studio</button>에 Google 계정으로 로그인 → <b>Get API key → Create API key</b> → 만들어진 키를 복사해 붙여 넣으세요. 결제 정보를 연결하지 않은 키는 무료 티어로 동작합니다.
                  </p>
                )}
                <div className="mt-2 flex gap-2">
                  <input type="password" autoComplete="off" spellCheck={false} className={inputCls} placeholder={gemini.key_stored ? '새 키로 바꾸려면 붙여 넣기' : 'AIza로 시작하는 API 키 붙여 넣기'} value={keyInput} onChange={(e) => setKeyInput(e.target.value)} />
                  <Button kind="primary" className="shrink-0 whitespace-nowrap" disabled={busy || keyInput.trim().length < 20} onClick={saveKey}>{busy ? '확인 중…' : '확인 후 저장'}</Button>
                </div>
                {avail && <p className="mt-2 text-xs">{avail.map((a) => <span key={a.id} className="mr-3">{geminiName(a.id)}: {a.available ? <b className="text-teal">사용 가능</b> : <b className="text-danger">이 키로 사용 불가</b>}</span>)}</p>}
              </div>
              <div>
                <h4 className="mb-1 text-sm font-semibold text-navy">2. 모델 선택</h4>
                <div className="grid gap-2 md:grid-cols-2">
                  {gemini.models.map((m: { id: string; label: string }) => {
                    const lim = settings.gemini_limits[m.id];
                    return (
                      <label key={m.id} className={cx('flex cursor-pointer items-start gap-2 rounded-lg border p-3 text-sm', settings.gemini_model === m.id ? 'border-violet/50 bg-violet-soft/40' : 'border-line bg-white')}>
                        <input type="radio" name="gm" className="mt-1 accent-[#7155A4]" checked={settings.gemini_model === m.id} onChange={() => save({ gemini_model: m.id })} />
                        <span><b className="text-navy">{m.id}</b><span className="block text-xs text-muted">{m.label.split('— ')[1]}</span><span className="text-xs">무료 한도: 분당 {lim?.rpm}회 · 하루 {lim?.rpd?.toLocaleString()}회</span></span>
                      </label>
                    );
                  })}
                </div>
                <p className="mt-1 text-xs text-muted">한도는 공개 자료 기준 기본값입니다. AI Studio의 실제 한도와 다르면 아래 “한도 수정”으로 고쳐 주세요. 논문 비교는 논문 1편당 1회 요청을 사용합니다.</p>
              </div>
              {gemini.key_stored && !usingGemini && <Button kind="primary" onClick={() => save({ provider: 'gemini' })}>Gemini로 답변 생성</Button>}
              <div>
                <h4 className="mb-2 text-sm font-semibold text-navy">3. 오늘의 무료 사용량</h4>
                <UsageCard usage={usage} settings={settings} onLimits={setLimits} />
              </div>
              <button className="text-xs text-muted underline" onClick={() => api.geminiConsent(false).then(() => { setView('ollama'); onChanged(); toast('동의를 철회하고 로컬 AI로 되돌렸습니다', 'ok'); })}>데이터 전송 동의 철회</button>
            </>
          )}
        </div>
      )}
      <ConsentModal open={consentOpen} onClose={() => setConsentOpen(false)} onAccept={accept} />
    </Card>
  );
}

/** 검색·비교 화면 상단의 클라우드 사용 안내 */
export function CloudBanner({ settings, usage }: { settings: Settings | null; usage: TodayUsage | null }) {
  if (!settings || settings.provider !== 'gemini') return null;
  const m = usage?.models.find((x) => x.model === settings.gemini_model);
  return (
    <div className="mb-4 flex flex-wrap items-center gap-2 rounded-lg border border-violet/30 bg-violet-soft/60 px-4 py-2.5 text-sm text-[#4b3a72]">
      <Badge tone="violet">Gemini API 사용 중</Badge>
      <span>{geminiName(settings.gemini_model)} · 질문과 관련 문단이 Google로 전송됩니다.</span>
      {m && <span className="ml-auto">오늘 무료 사용 <b>{m.requests}</b> / {m.limits.rpd}회 (남음 {m.remaining})</span>}
    </div>
  );
}
