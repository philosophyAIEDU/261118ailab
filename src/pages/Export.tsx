import { useEffect, useState } from 'react';
import { ExportButtons } from '../components/common';
import { api, errMsg, type Comparison, type Project, type Review } from '../lib/api';
import { pickFolder } from '../lib/dialogs';
import { fmtDate } from '../lib/format';
import { useNav } from '../lib/nav';
import { Button, Card, cx, inputCls, PageHeader, useToast } from '../lib/ui';

function Choose<T extends { id: string }>({ items, label, value, onChange }: { items: T[]; label: (x: T) => string; value: string; onChange: (v: string) => void }) {
  return (
    <select className={cx(inputCls, 'mb-3')} value={value} onChange={(e) => onChange(e.target.value)}>
      <option value="">{items.length ? '선택…' : '항목이 없습니다'}</option>
      {items.map((x) => <option key={x.id} value={x.id}>{label(x)}</option>)}
    </select>
  );
}

export default function ExportPage() {
  const { go } = useNav();
  const toast = useToast();
  const [cmp, setCmp] = useState<Comparison[]>([]);
  const [rev, setRev] = useState<Review[]>([]);
  const [prj, setPrj] = useState<Project[]>([]);
  const [srch, setSrch] = useState<any[]>([]);
  const [c, setC] = useState('');
  const [r, setR] = useState('');
  const [p, setP] = useState('');
  const [s, setS] = useState('');
  useEffect(() => {
    api.compareList().then(setCmp);
    api.reviewList().then(setRev);
    api.projects().then(setPrj);
    api.searchesRecent(50).then(setSrch);
  }, []);
  const obsidian = async () => {
    const dir = await pickFolder('Obsidian 보관함(Vault) 폴더 선택');
    if (!dir) return;
    try {
      const rep = await api.exportObsidian(dir);
      toast(`Obsidian으로 내보냈습니다: 노트 ${rep.notes_written}개, 논문 ${rep.papers_written}개${rep.skipped_existing ? ` (이미 있는 파일 ${rep.skipped_existing}개는 덮어쓰지 않음)` : ''}`, 'ok');
    } catch (e) {
      toast(errMsg(e), 'error');
    }
  };
  return (
    <div>
      <PageHeader title="내보내기" subtitle="연구 결과를 실제 파일로 저장합니다. 모든 파일에 생성 시각, 출처(논문·쪽), 검증 상태가 함께 기록됩니다." />
      <div className="grid gap-5 md:grid-cols-2">
        <Card title="논문 목록"><p className="mb-3 text-sm text-muted">전체 논문의 서지정보·키워드·본문 상태·메모</p><ExportButtons target={{ target: 'papers' }} formats={['xlsx', 'csv', 'docx', 'md', 'txt', 'bib']} name="논문 목록" /></Card>
        <Card title="연구 노트">
          <p className="mb-3 text-sm text-muted">전체 노트를 한 파일로, 또는 Obsidian 보관함 폴더로 (YAML Frontmatter, 태그, 관련 논문 링크, 프로젝트별 폴더). 기존 Obsidian 파일은 덮어쓰지 않습니다.</p>
          <div className="flex flex-wrap gap-2"><ExportButtons target={{ target: 'notes' }} formats={['md', 'docx', 'txt']} name="연구 노트" /><Button kind="primary" className="px-2.5 py-1.5 text-xs" onClick={obsidian}>Obsidian 폴더로 내보내기</Button></div>
        </Card>
        <Card title="논문 비교표">
          <Choose items={cmp} label={(x) => `${x.title} (${fmtDate(x.created_at)})`} value={c} onChange={setC} />
          {c ? <ExportButtons target={{ target: 'comparison', id: c }} formats={['xlsx', 'docx', 'md', 'csv']} name={cmp.find((x) => x.id === c)?.title ?? '비교표'} /> : <button className="text-xs text-violet underline" onClick={() => go('compare')}>비교표 만들기 →</button>}
        </Card>
        <Card title="문헌고찰 개요">
          <Choose items={rev} label={(x) => `${x.title} (${fmtDate(x.created_at)})`} value={r} onChange={setR} />
          {r ? <ExportButtons target={{ target: 'review', id: r }} formats={['docx', 'md', 'txt']} name={rev.find((x) => x.id === r)?.title ?? '문헌고찰'} /> : <button className="text-xs text-violet underline" onClick={() => go('review')}>개요 만들기 →</button>}
        </Card>
        <Card title="연구 프로젝트 보고서">
          <Choose items={prj} label={(x) => x.name} value={p} onChange={setP} />
          {p && <ExportButtons target={{ target: 'project', id: p }} formats={['docx', 'md', 'txt']} name={`프로젝트 보고서 - ${prj.find((x) => x.id === p)?.name}`} />}
        </Card>
        <Card title="AI 연구 검색 결과">
          <Choose items={srch} label={(x) => `${x.question.slice(0, 50)} (${fmtDate(x.created_at)})`} value={s} onChange={setS} />
          {s && <ExportButtons target={{ target: 'search', id: s }} formats={['docx', 'md', 'txt']} name="AI 검색 결과" />}
        </Card>
      </div>
      <p className="mt-6 text-xs text-muted">연구자료 전체를 보관하려면 설정 &gt; 연구실 자료의 <button className="underline" onClick={() => go('settings', { tab: 'data' })}>백업</button>을 사용하세요.</p>
    </div>
  );
}
