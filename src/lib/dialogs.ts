import { open, save } from '@tauri-apps/plugin-dialog';
import type { ExportFormat } from './api';

const FILTER: Record<ExportFormat, { name: string; extensions: string[] }> = {
  docx: { name: 'Word 문서', extensions: ['docx'] },
  xlsx: { name: 'Excel 통합 문서', extensions: ['xlsx'] },
  csv: { name: 'CSV', extensions: ['csv'] },
  md: { name: 'Markdown', extensions: ['md'] },
  txt: { name: '텍스트', extensions: ['txt'] },
  bib: { name: 'BibTeX', extensions: ['bib'] },
};

export async function pickDocuments(): Promise<string[]> {
  const r = await open({ multiple: true, title: '등록할 논문·자료 선택', filters: [{ name: '논문·자료', extensions: ['pdf', 'docx', 'txt', 'md', 'markdown', 'bib', 'ris'] }] });
  return r ? (Array.isArray(r) ? r : [r]) : [];
}
export async function pickFolder(title = '폴더 선택'): Promise<string | null> {
  const r = await open({ directory: true, multiple: false, title });
  return typeof r === 'string' ? r : null;
}
export async function pickBib(): Promise<string[]> {
  const r = await open({ multiple: true, title: 'Zotero에서 내보낸 서지 파일 선택', filters: [{ name: 'BibTeX / RIS', extensions: ['bib', 'ris'] }] });
  return r ? (Array.isArray(r) ? r : [r]) : [];
}
export async function pickBackup(): Promise<string | null> {
  const r = await open({ multiple: false, title: '백업 파일 선택', filters: [{ name: '연구실 백업', extensions: ['prhbak', 'enc'] }] });
  return typeof r === 'string' ? r : null;
}
export async function saveAs(fmt: ExportFormat, defaultName: string): Promise<string | null> {
  const safe = defaultName.replace(/[\\/:*?"<>|]/g, ' ').slice(0, 80);
  const r = await save({ title: '저장 위치 선택', defaultPath: `${safe}.${fmt}`, filters: [FILTER[fmt]] });
  if (!r) return null;
  return r.toLowerCase().endsWith(`.${fmt}`) ? r : `${r}.${fmt}`;
}
export async function saveBackup(encrypted: boolean): Promise<string | null> {
  const d = new Date();
  const stamp = `${d.getFullYear()}${String(d.getMonth() + 1).padStart(2, '0')}${String(d.getDate()).padStart(2, '0')}`;
  const ext = encrypted ? 'enc' : 'prhbak';
  const r = await save({ title: '백업 파일 저장', defaultPath: `연구실백업-${stamp}.${encrypted ? 'prhbak.enc' : 'prhbak'}`, filters: [{ name: '연구실 백업', extensions: [ext] }] });
  return r ?? null;
}
