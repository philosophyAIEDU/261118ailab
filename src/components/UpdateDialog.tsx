import { useEffect, useState } from 'react';
import { api, errMsg, newJob, onJob } from '../lib/api';
import { fmtBytes } from '../lib/format';
import { useNav } from '../lib/nav';
import { Button, Modal, Progress } from '../lib/ui';

/** 업데이트 안내 창. auto=true이면 실행 시 한 번 확인한다 (설정에서 끌 수 있음). */
export function UpdateDialog({ auto, info: given, onClose }: { auto?: boolean; info?: any; onClose?: () => void }) {
  const { info: app } = useNav();
  const [u, setU] = useState<any>(given ?? null);
  const [open, setOpen] = useState(!!given);
  const [stage, setStage] = useState<'ask' | 'downloading' | 'installed' | 'error'>('ask');
  const [prog, setProg] = useState({ done: 0, total: 0 });
  const [err, setErr] = useState('');

  useEffect(() => {
    if (!auto) return;
    let alive = true;
    (async () => {
      try {
        const s = await api.settings();
        if (!s.auto_check_updates) return;
        const r = await api.updateCheck(s.update_channel);
        if (alive && r.available) {
          setU(r);
          setOpen(true);
        }
      } catch {
        /* 오프라인 등: 조용히 넘어간다 */
      }
    })();
    return () => {
      alive = false;
    };
  }, [auto]);

  const close = () => {
    setOpen(false);
    onClose?.();
  };
  const install = async () => {
    const job = newJob();
    const un = await onJob(job, (e) => setProg({ done: e.done, total: e.total }));
    setStage('downloading');
    try {
      await api.updateInstall(job);
      setStage('installed');
    } catch (e) {
      setErr(errMsg(e));
      setStage('error');
    } finally {
      un();
    }
  };
  if (!u) return null;
  return (
    <Modal open={open} onClose={stage === 'downloading' ? () => {} : close} title="새로운 업데이트가 있습니다."
      footer={
        stage === 'ask' ? (<><Button onClick={close}>나중에</Button><Button kind="primary" onClick={install}>업데이트 설치</Button></>)
          : stage === 'installed' ? (<><Button onClick={close}>나중에 다시 시작</Button><Button kind="primary" onClick={() => api.restart()}>지금 다시 시작</Button></>)
            : stage === 'error' ? <Button onClick={close}>닫기</Button> : null
      }>
      <p className="mb-3 text-sm text-ink">교수님만의 AI 연구실의 새로운 버전이 출시되었습니다.</p>
      <dl className="mb-4 grid grid-cols-[120px_1fr] gap-y-1.5 text-sm">
        <dt className="text-muted">현재 버전</dt><dd>v{u.current ?? app?.version}</dd>
        <dt className="text-muted">최신 버전</dt><dd className="font-semibold text-navy">v{u.latest}{u.channel === 'beta' ? ' (시험판)' : ''}</dd>
        <dt className="text-muted">업데이트 파일 크기</dt><dd>{fmtBytes(u.size_bytes)}</dd>
      </dl>
      {u.features?.length > 0 && (<><h4 className="mb-1 text-sm font-semibold text-navy">개선된 기능</h4><ul className="mb-3 list-disc pl-5 text-sm">{u.features.map((f: string) => <li key={f}>{f}</li>)}</ul></>)}
      {u.fixes?.length > 0 && (<><h4 className="mb-1 text-sm font-semibold text-navy">수정된 오류</h4><ul className="mb-3 list-disc pl-5 text-sm">{u.fixes.map((f: string) => <li key={f}>{f}</li>)}</ul></>)}
      {!u.features?.length && !u.fixes?.length && u.notes && <pre className="mb-3 text-sm whitespace-pre-wrap">{u.notes}</pre>}
      <p className="rounded-lg bg-ivory px-3 py-2 text-xs text-muted">설치 전에 연구실 데이터베이스의 안전 사본을 만듭니다. 업데이트 파일은 전자서명을 검증한 뒤에만 설치되며, 연구자료(논문·노트·설정)는 프로그램과 별도 폴더에 있어 그대로 유지됩니다.</p>
      {stage === 'downloading' && <div className="mt-4"><Progress done={prog.done} total={prog.total} label={`내려받는 중 ${fmtBytes(prog.done)} / ${fmtBytes(prog.total || null)}`} /></div>}
      {stage === 'installed' && <p className="mt-4 text-sm font-semibold text-teal">설치를 마쳤습니다. 프로그램을 다시 시작하면 새 버전이 적용됩니다.</p>}
      {stage === 'error' && <p className="mt-4 text-sm text-danger">{err}</p>}
    </Modal>
  );
}
