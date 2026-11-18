import { useEffect, useMemo, useState } from 'react';
import { api, errMsg, type ExportFormat, type ExportTarget, type Paper, type Source } from '../lib/api';
import { saveAs } from '../lib/dialogs';
import { authorsShort } from '../lib/format';
import { useOpenSource } from '../lib/nav';
import { Badge, Button, cx, inputCls, useToast } from '../lib/ui';

const FMT_LABEL: Record<ExportFormat, string> = { docx: 'Word', xlsx: 'Excel', csv: 'CSV', md: 'Markdown', txt: 'TXT', bib: 'BibTeX' };

/** 실제 파일을 만드는 내보내기 버튼 묶음 */
export function ExportButtons({ target, formats, name }: { target: ExportTarget; formats: ExportFormat[]; name: string }) {
  const toast = useToast();
  const [busy, setBusy] = useState<ExportFormat | null>(null);
  const run = async (f: ExportFormat) => {
    const path = await saveAs(f, name);
    if (!path) return;
    setBusy(f);
    try {
      const out = await api.exportFile(target, f, path);
      toast(`저장했습니다: ${out}`, 'ok');
    } catch (e) {
      toast(errMsg(e), 'error');
    } finally {
      setBusy(null);
    }
  };
  return (
    <div className="flex flex-wrap gap-1.5">
      {formats.map((f) => (
        <Button key={f} kind="secondary" className="px-2.5 py-1.5 text-xs" disabled={busy !== null} onClick={() => run(f)}>
          {busy === f ? '저장 중…' : `${FMT_LABEL[f]}로 저장`}
        </Button>
      ))}
    </div>
  );
}

/** 논문 여러 편 선택 */
export function PaperPicker({ selected, onChange, max, filter }: { selected: string[]; onChange: (ids: string[]) => void; max?: number; filter?: (p: Paper) => boolean }) {
  const [papers, setPapers] = useState<Paper[]>([]);
  const [q, setQ] = useState('');
  useEffect(() => {
    api.papers({ sort: 'title' }).then(setPapers).catch(() => {});
  }, []);
  const list = useMemo(() => {
    const ql = q.trim().toLowerCase();
    return papers.filter((p) => (!filter || filter(p)) && (!ql || p.title.toLowerCase().includes(ql) || p.authors.join(' ').toLowerCase().includes(ql) || p.keywords.join(' ').toLowerCase().includes(ql)));
  }, [papers, q, filter]);
  const toggle = (id: string) => {
    if (selected.includes(id)) onChange(selected.filter((x) => x !== id));
    else if (!max || selected.length < max) onChange([...selected, id]);
  };
  return (
    <div>
      <input className={inputCls} placeholder="제목·저자·키워드로 찾기" value={q} onChange={(e) => setQ(e.target.value)} />
      <div className="mt-1.5 mb-1 flex justify-between text-xs text-muted">
        <span>{selected.length}편 선택{max ? ` (최대 ${max}편)` : ''}</span>
        {selected.length > 0 && <button className="underline" onClick={() => onChange([])}>선택 해제</button>}
      </div>
      <div className="scroll-thin max-h-80 overflow-y-auto rounded-lg border border-line bg-white">
        {list.length === 0 && <p className="p-4 text-sm text-muted">표시할 논문이 없습니다.</p>}
        {list.map((p) => {
          const on = selected.includes(p.id);
          const noText = !['ok', 'partial'].includes(p.text_status);
          return (
            <label key={p.id} className={cx('flex cursor-pointer items-start gap-2.5 border-b border-line/70 px-3 py-2 last:border-0 hover:bg-ivory/60', on && 'bg-violet-soft/60')}>
              <input type="checkbox" className="mt-1 accent-[#7155A4]" checked={on} onChange={() => toggle(p.id)} />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-medium text-ink">{p.title}</span>
                <span className="text-xs text-muted">{authorsShort(p.authors)} · {p.year ?? '연도 미상'}</span>
              </span>
              {noText && <Badge tone="danger">본문 없음</Badge>}
            </label>
          );
        })}
      </div>
    </div>
  );
}

/** 원문 근거 카드 (AI 답변과 구분되는 "원문") */
export function SourceCard({ s, badge, compact }: { s: Source; badge?: React.ReactNode; compact?: boolean }) {
  const open = useOpenSource();
  return (
    <div id={`src-${s.label}`} className="rounded-lg border border-line bg-white p-3">
      <div className="mb-1.5 flex flex-wrap items-center gap-1.5">
        <Badge tone="navy">{s.label}</Badge>
        {badge}
        {s.suspicious && <Badge tone="danger" title="문서 안에 AI에게 지시하는 듯한 문장이 있습니다. 답변에 반영하지 않도록 처리했습니다.">주의: 지시문 포함</Badge>}
        <button className="min-w-0 truncate text-left text-sm font-semibold text-navy hover:underline" onClick={() => open(s.paper_id, s.page, s.text)} title="원문 쪽 열기">
          {s.paper_title}
        </button>
        <span className="text-xs text-muted">{s.year ?? ''}</span>
      </div>
      <p className={cx('border-l-2 border-gold/60 pl-2.5 text-[13px] text-[#3b3f48]', compact && 'line-clamp-3')}>
        <span className="mr-1 text-[11px] font-semibold text-gold">원문</span>
        {s.text}
      </p>
      <div className="mt-2">
        <Button kind="ghost" className="px-2 py-1 text-xs" onClick={() => open(s.paper_id, s.page, s.text)}>
          {s.page ? `PDF ${s.page}쪽 열기 →` : '원문 열기 →'}
        </Button>
      </div>
    </div>
  );
}
