import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react';

export function cx(...a: (string | false | null | undefined)[]) {
  return a.filter(Boolean).join(' ');
}

type BtnKind = 'primary' | 'secondary' | 'ghost' | 'danger' | 'gold';
export function Button({ kind = 'secondary', className, ...p }: React.ButtonHTMLAttributes<HTMLButtonElement> & { kind?: BtnKind }) {
  const k: Record<BtnKind, string> = {
    primary: 'bg-navy text-white hover:bg-navy-soft border border-navy',
    secondary: 'bg-paper text-navy border border-line hover:border-navy/40',
    ghost: 'text-navy hover:bg-navy/5 border border-transparent',
    danger: 'bg-paper text-danger border border-danger/40 hover:bg-danger/5',
    gold: 'bg-gold text-navy border border-gold hover:brightness-105 font-semibold',
  };
  return <button {...p} className={cx('inline-flex items-center justify-center gap-1.5 rounded-lg px-3.5 py-2 text-sm font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50', k[kind], className)} />;
}

export function Card({ children, className, title, action }: { children: ReactNode; className?: string; title?: ReactNode; action?: ReactNode }) {
  return (
    <section className={cx('rounded-xl border border-line bg-paper p-5', className)}>
      {(title || action) && (
        <div className="mb-3 flex items-center justify-between gap-3">
          {title && <h3 className="text-[15px] font-semibold text-navy">{title}</h3>}
          {action}
        </div>
      )}
      {children}
    </section>
  );
}

type Tone = 'navy' | 'violet' | 'gold' | 'teal' | 'muted' | 'danger';
export function Badge({ tone = 'muted', children, title }: { tone?: Tone; children: ReactNode; title?: string }) {
  const t: Record<Tone, string> = {
    navy: 'bg-navy/8 text-navy border-navy/15',
    violet: 'bg-violet-soft text-violet border-violet/20',
    gold: 'bg-gold-soft text-[#8a6a2c] border-gold/30',
    teal: 'bg-teal-soft text-teal border-teal/25',
    muted: 'bg-[#f0ece3] text-muted border-line',
    danger: 'bg-danger/8 text-danger border-danger/25',
  };
  return <span title={title} className={cx('inline-flex items-center rounded-md border px-1.5 py-0.5 text-[11.5px] font-medium leading-tight whitespace-nowrap', t[tone])}>{children}</span>;
}

export function PageHeader({ title, subtitle, actions }: { title: string; subtitle?: string; actions?: ReactNode }) {
  return (
    <header className="mb-5 flex flex-wrap items-end justify-between gap-3">
      <div>
        <h1 className="text-[22px] font-bold tracking-tight text-navy">{title}</h1>
        {subtitle && <p className="mt-0.5 text-sm text-muted">{subtitle}</p>}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </header>
  );
}

export function Empty({ title, children, action }: { title: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="rounded-xl border border-dashed border-line bg-paper/60 px-6 py-10 text-center">
      <p className="font-semibold text-navy">{title}</p>
      {children && <div className="mx-auto mt-1 max-w-lg text-sm text-muted">{children}</div>}
      {action && <div className="mt-4 flex justify-center gap-2">{action}</div>}
    </div>
  );
}

export function Spinner({ label }: { label?: string }) {
  return (
    <span className="inline-flex items-center gap-2 text-sm text-muted">
      <span className="h-3.5 w-3.5 animate-spin rounded-full border-2 border-line border-t-violet" />
      {label}
    </span>
  );
}

export function Progress({ done, total, label }: { done: number; total: number; label?: string }) {
  const pct = total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;
  return (
    <div>
      {label && <div className="mb-1 flex justify-between text-xs text-muted"><span>{label}</span><span>{total > 0 ? `${pct}%` : ''}</span></div>}
      <div className="h-2 overflow-hidden rounded-full bg-[#ebe5d8]"><div className="h-full bg-teal transition-all" style={{ width: `${pct}%` }} /></div>
    </div>
  );
}

export function Modal({ open, onClose, title, children, footer, wide }: { open: boolean; onClose: () => void; title: string; children: ReactNode; footer?: ReactNode; wide?: boolean }) {
  useEffect(() => {
    if (!open) return;
    const h = (e: KeyboardEvent) => e.key === 'Escape' && onClose();
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, [open, onClose]);
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-navy/35 p-6" onMouseDown={onClose}>
      <div role="dialog" aria-modal="true" aria-label={title} className={cx('flex max-h-[88vh] w-full flex-col rounded-xl border border-line bg-paper', wide ? 'max-w-4xl' : 'max-w-xl')} onMouseDown={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between border-b border-line px-5 py-3.5">
          <h2 className="text-base font-semibold text-navy">{title}</h2>
          <button aria-label="닫기" className="rounded px-2 text-lg text-muted hover:text-navy" onClick={onClose}>×</button>
        </div>
        <div className="scroll-thin overflow-y-auto px-5 py-4">{children}</div>
        {footer && <div className="flex justify-end gap-2 border-t border-line px-5 py-3">{footer}</div>}
      </div>
    </div>
  );
}

export function Field({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1 block text-[13px] font-medium text-navy">{label}</span>
      {children}
      {hint && <span className="mt-1 block text-xs text-muted">{hint}</span>}
    </label>
  );
}

export const inputBase = 'rounded-lg border border-line bg-white px-3 py-2 text-sm text-ink placeholder:text-[#a3a39f] focus:border-violet focus:outline-none';
export const inputCls = `w-full ${inputBase}`;

export function Tabs<T extends string>({ value, onChange, items }: { value: T; onChange: (v: T) => void; items: { key: T; label: string }[] }) {
  return (
    <div role="tablist" className="mb-4 inline-flex rounded-lg border border-line bg-paper p-0.5">
      {items.map((i) => (
        <button key={i.key} role="tab" aria-selected={value === i.key} onClick={() => onChange(i.key)} className={cx('rounded-md px-3.5 py-1.5 text-sm', value === i.key ? 'bg-navy text-white' : 'text-navy hover:bg-navy/5')}>
          {i.label}
        </button>
      ))}
    </div>
  );
}

// ── 알림(토스트) ──
type Toast = { id: number; text: string; tone: 'ok' | 'error' | 'info' };
const ToastCtx = createContext<(text: string, tone?: Toast['tone']) => void>(() => {});
export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<Toast[]>([]);
  const push = useCallback((text: string, tone: Toast['tone'] = 'info') => {
    const id = Date.now() + Math.random();
    setItems((s) => [...s, { id, text, tone }]);
    setTimeout(() => setItems((s) => s.filter((t) => t.id !== id)), tone === 'error' ? 8000 : 3000);
  }, []);
  return (
    <ToastCtx.Provider value={push}>
      {children}
      <div className="pointer-events-none fixed right-5 bottom-5 z-[60] flex w-96 flex-col gap-2">
        {items.map((t) => (
          <div key={t.id} role="status" className={cx('pointer-events-none rounded-lg border px-4 py-3 text-sm', t.tone === 'error' ? 'border-danger/40 bg-[#fbefed] text-danger' : t.tone === 'ok' ? 'border-teal/40 bg-teal-soft text-[#2f5f5b]' : 'border-line bg-paper text-navy')}>
            {t.text}
          </div>
        ))}
      </div>
    </ToastCtx.Provider>
  );
}
export const useToast = () => useContext(ToastCtx);
