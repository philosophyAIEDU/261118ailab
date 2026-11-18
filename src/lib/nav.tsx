import { createContext, useContext } from 'react';

export type Page = 'dashboard' | 'papers' | 'search' | 'compare' | 'review' | 'graph' | 'notes' | 'projects' | 'zotero' | 'export' | 'settings' | 'viewer';
export interface Route { page: Page; params?: Record<string, any> }
export const NavCtx = createContext<{ route: Route; go: (page: Page, params?: Record<string, any>) => void; refreshInfo: () => void; info: any }>({ route: { page: 'dashboard' }, go: () => {}, refreshInfo: () => {}, info: null });
export const useNav = () => useContext(NavCtx);
/** 출처(논문·쪽)로 이동 */
export function useOpenSource() {
  const { go } = useNav();
  return (paperId: string, page: number | null | undefined, highlight?: string) => go('viewer', { paperId, page: page ?? 1, highlight });
}
