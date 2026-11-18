import { ask as confirmDlg } from '@tauri-apps/plugin-dialog';
import { useEffect, useState } from 'react';
import { ExportButtons, PaperPicker } from '../components/common';
import { api, errMsg, type Note, type Paper, type Project } from '../lib/api';
import { authorsShort, fmtDate } from '../lib/format';
import { useNav } from '../lib/nav';
import { Badge, Button, Card, cx, Empty, Field, inputCls, Modal, PageHeader, useToast } from '../lib/ui';

const STATUS: Record<string, string> = { active: '진행 중', paused: '보류', done: '완료' };

export default function Projects() {
  const { route, go } = useNav();
  const toast = useToast();
  const [list, setList] = useState<Project[]>([]);
  const [sel, setSel] = useState<string | null>(route.params?.id ?? null);
  const [edit, setEdit] = useState<{ id: string | null; name: string; description: string; status: string } | null>(null);
  const [pick, setPick] = useState<string[] | null>(null);
  const [papers, setPapers] = useState<Paper[]>([]);
  const [notes, setNotes] = useState<Note[]>([]);
  const load = () => api.projects().then(setList);
  useEffect(() => { load(); api.papers({ sort: 'title' }).then(setPapers); }, []);
  const cur = list.find((p) => p.id === sel) ?? null;
  useEffect(() => { if (sel) api.notes({ project_id: sel }).then(setNotes); }, [sel, list]);
  const save = async () => {
    if (!edit) return;
    try {
      const p = await api.projectSave(edit.id, { name: edit.name, description: edit.description, status: edit.status });
      setEdit(null);
      await load();
      setSel(p.id);
    } catch (e) {
      toast(errMsg(e), 'error');
    }
  };
  const applyPick = async () => {
    if (!cur || !pick) return;
    const add = pick.filter((x) => !cur.paper_ids.includes(x));
    const remove = cur.paper_ids.filter((x) => !pick.includes(x));
    await api.projectPapers(cur.id, add, remove);
    setPick(null);
    load();
  };
  return (
    <div>
      <PageHeader title="연구 프로젝트" subtitle="논문과 노트를 연구 주제별로 묶어 관리하고, 프로젝트 보고서로 내보낼 수 있습니다."
        actions={<Button kind="primary" onClick={() => setEdit({ id: null, name: '', description: '', status: 'active' })}>새 프로젝트</Button>} />
      <div className="grid gap-5 lg:grid-cols-[320px_1fr]">
        <div className="space-y-2">
          {list.length === 0 && <Empty title="프로젝트가 없습니다" />}
          {list.map((p) => (
            <button key={p.id} onClick={() => setSel(p.id)} className={cx('w-full rounded-xl border bg-paper p-3 text-left', sel === p.id ? 'border-violet/50' : 'border-line hover:border-navy/30')}>
              <div className="flex items-center gap-2"><span className="min-w-0 flex-1 truncate font-medium">{p.name}</span><Badge tone={p.status === 'active' ? 'teal' : 'muted'}>{STATUS[p.status]}</Badge></div>
              <div className="text-xs text-muted">논문 {p.paper_ids.length}편 · 노트 {p.note_count}개 · {fmtDate(p.updated_at)}</div>
            </button>
          ))}
        </div>
        {!cur ? <Empty title="프로젝트를 선택하세요" /> : (
          <Card title={cur.name} action={<div className="flex gap-1.5"><Button kind="ghost" onClick={() => setEdit({ id: cur.id, name: cur.name, description: cur.description, status: cur.status })}>편집</Button><Button kind="danger" onClick={async () => { if (await confirmDlg(`「${cur.name}」 프로젝트를 삭제할까요? 논문과 노트는 삭제되지 않습니다.`, { title: '프로젝트 삭제', kind: 'warning', okLabel: '삭제', cancelLabel: '취소' })) { await api.projectDelete(cur.id); setSel(null); load(); } }}>삭제</Button></div>}>
            {cur.description && <p className="mb-4 text-sm whitespace-pre-wrap text-[#3b3f48]">{cur.description}</p>}
            <div className="mb-4 flex flex-wrap gap-2">
              <Button onClick={() => go('search', { scope: cur.paper_ids })} disabled={!cur.paper_ids.length}>이 프로젝트 논문에 질문</Button>
              <Button onClick={() => go('compare')}>논문 비교</Button>
              <Button onClick={() => go('review')}>문헌고찰 개요</Button>
            </div>
            <div className="mb-2 flex items-center justify-between"><h4 className="font-semibold text-navy">연구 자료 ({cur.paper_ids.length}편)</h4><Button kind="ghost" className="text-xs" onClick={() => setPick(cur.paper_ids)}>논문 추가·제외</Button></div>
            <ul className="mb-5 divide-y divide-line/70 rounded-lg border border-line">
              {cur.paper_ids.length === 0 && <li className="p-3 text-sm text-muted">연결된 논문이 없습니다.</li>}
              {cur.paper_ids.map((id) => { const p = papers.find((x) => x.id === id); return p && <li key={id} className="px-3 py-2"><button className="text-left text-sm hover:text-violet" onClick={() => go('papers', { id })}>{p.title}</button><div className="text-xs text-muted">{authorsShort(p.authors)} · {p.year ?? '연도 미상'}</div></li>; })}
            </ul>
            <h4 className="mb-2 font-semibold text-navy">연구 노트 ({notes.length}개)</h4>
            <ul className="mb-5 space-y-1">{notes.length === 0 ? <li className="text-sm text-muted">노트 화면에서 이 프로젝트를 지정하면 여기에 모입니다.</li> : notes.map((n) => <li key={n.id}><button className="text-sm text-violet hover:underline" onClick={() => go('notes', { id: n.id })}>{n.title}</button></li>)}</ul>
            <h4 className="mb-2 font-semibold text-navy">프로젝트 보고서</h4>
            <ExportButtons target={{ target: 'project', id: cur.id }} formats={['docx', 'md', 'txt']} name={`프로젝트 보고서 - ${cur.name}`} />
          </Card>
        )}
      </div>
      <Modal open={!!edit} onClose={() => setEdit(null)} title={edit?.id ? '프로젝트 편집' : '새 프로젝트'} footer={<><Button onClick={() => setEdit(null)}>취소</Button><Button kind="primary" onClick={save}>저장</Button></>}>
        {edit && (
          <div className="space-y-3">
            <Field label="이름"><input className={inputCls} value={edit.name} onChange={(e) => setEdit({ ...edit, name: e.target.value })} /></Field>
            <Field label="설명"><textarea className={cx(inputCls, 'min-h-24')} value={edit.description} onChange={(e) => setEdit({ ...edit, description: e.target.value })} /></Field>
            <Field label="상태"><select className={inputCls} value={edit.status} onChange={(e) => setEdit({ ...edit, status: e.target.value })}>{Object.entries(STATUS).map(([k, v]) => <option key={k} value={k}>{v}</option>)}</select></Field>
          </div>
        )}
      </Modal>
      <Modal open={!!pick} onClose={() => setPick(null)} title="프로젝트 논문 선택" footer={<><Button onClick={() => setPick(null)}>취소</Button><Button kind="primary" onClick={applyPick}>적용</Button></>}>
        {pick && <PaperPicker selected={pick} onChange={setPick} />}
      </Modal>
    </div>
  );
}
