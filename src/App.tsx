import { useCallback, useEffect, useState } from 'react';
import { api, errMsg } from './lib/api';
import { NavCtx, type Page, type Route } from './lib/nav';
import { cx, ToastProvider } from './lib/ui';
import { UpdateDialog } from './components/UpdateDialog';
import Dashboard from './pages/Dashboard';
import Papers from './pages/Papers';
import Viewer from './pages/Viewer';
import Search from './pages/Search';
import Compare from './pages/Compare';
import Review from './pages/Review';
import Graph from './pages/Graph';
import Notes from './pages/Notes';
import Projects from './pages/Projects';
import Zotero from './pages/Zotero';
import ExportPage from './pages/Export';
import SettingsPage from './pages/Settings';

const NAV: { key: Page; label: string; icon: string }[] = [
  { key: 'dashboard', label: '대시보드', icon: '◧' },
  { key: 'papers', label: '내 논문', icon: '▤' },
  { key: 'search', label: 'AI 연구 검색', icon: '⌕' },
  { key: 'compare', label: '논문 비교', icon: '⇆' },
  { key: 'review', label: '문헌고찰 지원', icon: '☰' },
  { key: 'graph', label: '지식 그래프', icon: '✣' },
  { key: 'notes', label: '연구 노트', icon: '✎' },
  { key: 'projects', label: '연구 프로젝트', icon: '◈' },
  { key: 'zotero', label: 'Zotero 가져오기', icon: '⇣' },
  { key: 'export', label: '내보내기', icon: '⇡' },
  { key: 'settings', label: '설정', icon: '⚙' },
];

function Splash({ error }: { error?: string }) {
  return (
    <div className="flex h-full flex-col items-center justify-center bg-navy text-ivory">
      <img src="/icon.png" alt="" className="mb-5 h-20 w-20" />
      <h1 className="text-3xl font-bold tracking-tight">교수님만의 AI 연구실</h1>
      <p className="mt-2 text-sm text-ivory/70">논문을 모으고, 연구의 연결을 발견하세요.</p>
      <p className="mt-8 text-xs text-ivory/50">{error ?? '연구실을 여는 중…'}</p>
    </div>
  );
}

export default function App() {
  const [route, setRoute] = useState<Route>({ page: 'dashboard' });
  const [info, setInfo] = useState<any>(null);
  const [bootError, setBootError] = useState<string>();
  const [splash, setSplash] = useState(true);
  const refreshInfo = useCallback(() => {
    api.appInfo().then(setInfo).catch((e) => setBootError(errMsg(e)));
  }, []);
  useEffect(() => {
    refreshInfo();
    const t = setTimeout(() => setSplash(false), 700);
    return () => clearTimeout(t);
  }, [refreshInfo]);
  const go = useCallback((page: Page, params?: Record<string, any>) => {
    setRoute({ page, params });
    document.getElementById('main')?.scrollTo(0, 0);
  }, []);

  if (!info || splash) return <Splash error={bootError} />;
  const lib = info.library;
  const noLibrary = !lib;
  const page = noLibrary && route.page !== 'settings' ? 'settings' : route.page;

  return (
    <ToastProvider>
      <NavCtx.Provider value={{ route, go, refreshInfo, info }}>
        <div className="flex h-full">
          <aside className="flex w-60 shrink-0 flex-col bg-navy text-ivory">
            <div className="px-5 pt-5 pb-4">
              <div className="flex items-center gap-2.5">
                <img src="/icon.png" alt="" className="h-9 w-9" />
                <div>
                  <div className="text-[15px] leading-tight font-bold">교수님만의 AI 연구실</div>
                  <div className="text-[11px] text-ivory/55">Philosophy Research Hub</div>
                </div>
              </div>
            </div>
            <nav className="scroll-thin flex-1 overflow-y-auto px-2.5" aria-label="주 메뉴">
              {NAV.map((n) => (
                <button key={n.key} disabled={noLibrary && n.key !== 'settings'} onClick={() => go(n.key)}
                  className={cx('mb-0.5 flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-[14px] disabled:opacity-35',
                    page === n.key || (page === 'viewer' && n.key === 'papers') ? 'bg-ivory/12 font-semibold text-white' : 'text-ivory/80 hover:bg-ivory/7')}>
                  <span className="w-4 text-center text-gold" aria-hidden>{n.icon}</span>
                  {n.label}
                </button>
              ))}
            </nav>
            <div className="border-t border-ivory/10 px-5 py-3 text-[11px] leading-relaxed text-ivory/50">
              {lib?.demo && <div className="mb-1 rounded bg-gold/20 px-2 py-1 text-gold">데모 연구실 (가상 자료)</div>}
              <div>v{info.version} · 필로소피 AI 교육</div>
              <div>자료는 이 컴퓨터에만 저장됩니다</div>
            </div>
          </aside>
          <main id="main" className="scroll-thin min-w-0 flex-1 overflow-y-auto">
            <div className={cx('mx-auto px-8 py-7', page === 'viewer' || page === 'graph' ? 'max-w-none' : 'max-w-[1240px]')}>
              {page === 'dashboard' && <Dashboard />}
              {page === 'papers' && <Papers />}
              {page === 'viewer' && <Viewer key={`${route.params?.paperId}-${route.params?.page}-${route.params?.highlight ?? ''}`} />}
              {page === 'search' && <Search />}
              {page === 'compare' && <Compare />}
              {page === 'review' && <Review />}
              {page === 'graph' && <Graph />}
              {page === 'notes' && <Notes />}
              {page === 'projects' && <Projects />}
              {page === 'zotero' && <Zotero />}
              {page === 'export' && <ExportPage />}
              {page === 'settings' && <SettingsPage />}
            </div>
          </main>
        </div>
        {!noLibrary && <UpdateDialog auto />}
      </NavCtx.Provider>
    </ToastProvider>
  );
}
