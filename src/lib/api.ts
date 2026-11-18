// 백엔드(Rust) 명령 호출과 자료형. 모든 처리는 이 컴퓨터 안에서 이루어진다.
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type MetaSource = Record<string, string>;
export interface Paper {
  id: string; title: string; authors: string[]; year: number | null; journal: string; doi: string; abstract: string;
  keywords: string[]; field: string; memo: string; tags: string[]; file_path: string | null; original_name: string;
  file_kind: string; file_size: number; page_count: number | null; text_status: string; status_detail: string;
  meta_source: MetaSource; bib_key: string; created_at: string; updated_at: string; chunk_count: number;
}
export interface Stats { papers: number; papers_with_text: number; notes: number; projects_active: number; chunks: number; embedded_chunks: number; comparisons: number; needs_attention: number }
export interface Source { label: string; chunk_id: number; paper_id: string; paper_title: string; authors: string[]; year: number | null; page: number | null; text: string; suspicious: boolean }
export interface Citation { label: string; source: Source; support: 'verified' | 'weak'; overlap: number }
export type Mode = 'keyword' | 'semantic' | 'hybrid';
export interface AskResult {
  id: string; question: string; status: 'answered' | 'insufficient' | 'no_ai'; answer: string; citations: Citation[]; other_sources: Source[];
  removed_markers: number; removed_references: string[]; model: string; mode_used: Mode; notice: string | null; created_at: string;
}
export interface Hit { chunk_id: number; paper_id: string; paper_title: string; authors: string[]; year: number | null; page: number | null; text: string; score: number; keyword_rank: number | null; vector_rank: number | null; vector_sim: number | null }
export interface SearchOutcome { hits: Hit[]; mode_used: Mode; terms: string[]; notice: string | null }
export interface Note { id: string; title: string; body: string; project_id: string | null; origin: string; tags: string[]; paper_ids: string[]; created_at: string; updated_at: string }
export interface NoteRef { id: string; title: string }
export interface NoteDetail extends Note { backlinks: NoteRef[]; outgoing: NoteRef[]; unresolved: string[] }
export interface Project { id: string; name: string; description: string; status: string; paper_ids: string[]; note_count: number; created_at: string; updated_at: string }
export interface Evidence { chunk_id: number; page: number | null; quote: string }
export interface Cell { status: 'found' | 'weak' | 'not_found'; value: string; evidence: Evidence[] }
export interface PaperColumn { paper_id: string; title: string; authors: string[]; year: number | null; cells: Record<string, Cell>; error: string | null }
export interface Comparison { id: string; title: string; aspects: [string, string][]; columns: PaperColumn[]; model: string; created_at: string }
export interface ReviewCitation { label: string; paper_id: string; paper_title: string; page: number | null; chunk_id: number; support: string; quote: string }
export interface Review { id: string; title: string; topic: string; paper_ids: string[]; groups: { label: string; basis: string; paper_ids: string[] }[]; draft: string; citations: ReviewCitation[]; removed_markers: number; removed_references: string[]; references: string[]; model: string; created_at: string; notice: string }
export interface GNode { id: string; kind: 'paper' | 'note' | 'project' | 'topic' | 'concept'; label: string; ref_id: string; weight: number }
export interface GEdge { id: string; source: string; target: string; kind: 'citation' | 'topic' | 'keyword' | 'user' | 'ai_suggested'; label: string; score: number | null; page: number | null }
export interface Annotation { id: string; paper_id: string; page: number; kind: 'highlight' | 'memo' | 'bookmark'; text: string; note: string; color: string; rects: { x: number; y: number; w: number; h: number }[]; created_at: string }
export interface Settings { chat_model: string | null; embed_model: string | null; rerank_model: string | null; ollama_port: number; update_channel: 'stable' | 'beta'; auto_check_updates: boolean; cloud_ai_enabled: boolean; top_k: number }
export interface ImportOutcome { file_name: string; source_path: string; status: 'added' | 'duplicate' | 'failed' | 'updated'; paper_id: string | null; message: string; warnings: string[] }
export interface JobEvent { id: string; kind: string; stage: 'progress' | 'token' | 'done' | 'error'; done: number; total: number; message: string; data: any }
export type ExportFormat = 'docx' | 'xlsx' | 'csv' | 'md' | 'txt' | 'bib';
export type ExportTarget =
  | { target: 'papers'; ids?: string[] } | { target: 'comparison'; id: string } | { target: 'notes'; ids?: string[] }
  | { target: 'review'; id: string } | { target: 'project'; id: string } | { target: 'search'; id: string };

export interface ApiError { kind: string; message: string }
export function errMsg(e: unknown): string {
  if (e && typeof e === 'object' && 'message' in e) return String((e as ApiError).message);
  return String(e);
}

const call = <T,>(cmd: string, args?: Record<string, unknown>) => invoke<T>(cmd, args);
export const newJob = () => (crypto.randomUUID ? crypto.randomUUID() : String(Math.random()).slice(2));

export const api = {
  appInfo: () => call<any>('app_info'),
  libraryOpen: (path: string) => call<any>('library_open', { path }),
  libraryOpenDefault: () => call<any>('library_open_default'),
  demoOpen: (job: string) => call<{ root: string; created: boolean }>('demo_open', { job }),
  revealLibrary: () => call<void>('reveal_library'),
  openUrl: (url: string) => call<void>('open_url', { url }),
  dashboard: () => call<any>('dashboard'),
  papers: (filter: { query?: string; tag?: string | null; project_id?: string | null; status?: string | null; sort?: string | null } = {}) => call<Paper[]>('papers_list', { filter }),
  paper: (id: string) => call<{ paper: Paper; notes: Note[]; citations: any[]; projects: { id: string; name: string }[] }>('paper_get', { id }),
  paperUpdate: (id: string, update: any) => call<Paper>('paper_update', { id, update }),
  paperDelete: (ids: string[]) => call<number>('paper_delete', { ids }),
  paperReprocess: (id: string) => call<Paper>('paper_reprocess', { id }),
  papersTag: (ids: string[], add: string[], remove: string[]) => call<void>('papers_tag', { ids, add, remove }),
  tags: () => call<[string, number][]>('tags_list'),
  paperFile: (id: string) => call<ArrayBuffer>('paper_file', { id }),
  importStart: (paths: string[], job: string) => call<void>('import_start', { paths, job }),
  jobCancel: (job: string) => call<void>('job_cancel', { job }),
  annotations: (paperId: string) => call<Annotation[]>('annotations_list', { paperId }),
  annotationAdd: (input: Omit<Annotation, 'id' | 'created_at'>) => call<Annotation>('annotation_add', { input }),
  annotationUpdate: (id: string, note: string) => call<void>('annotation_update', { id, note }),
  annotationDelete: (id: string) => call<void>('annotation_delete', { id }),
  searchPassages: (query: string, mode: Mode, limit?: number, paperIds?: string[]) => call<SearchOutcome>('search_passages', { query, mode, limit, paperIds }),
  ask: (req: { question: string; mode: Mode; paper_ids: string[]; top_k: number }, job: string) => call<AskResult>('ask', { req, job }),
  searchesRecent: (limit?: number) => call<AskResult[]>('searches_recent', { limit }),
  searchDelete: (id: string) => call<void>('search_delete', { id }),
  settings: () => call<Settings>('settings_get'),
  settingsSave: (settings: Settings) => call<Settings>('settings_save', { settings }),
  aiStatus: () => call<any>('ai_status'),
  aiPull: (model: string, job: string) => call<void>('ai_pull', { model, job }),
  aiUnload: (model: string) => call<void>('ai_unload', { model }),
  aiDeleteModel: (model: string) => call<void>('ai_delete_model', { model }),
  indexBuild: (job: string) => call<number>('index_build', { job }),
  compareAspects: () => call<{ key: string; label: string }[]>('compare_aspects'),
  compareRun: (paperIds: string[], aspects: string[], title: string, job: string) => call<Comparison>('compare_run', { paperIds, aspects, title, job }),
  compareList: () => call<Comparison[]>('compare_list'),
  compareDelete: (id: string) => call<void>('compare_delete', { id }),
  notes: (filter: { query?: string; tag?: string | null; project_id?: string | null; paper_id?: string | null } = {}) => call<Note[]>('notes_list', { filter }),
  note: (id: string) => call<NoteDetail>('note_get', { id }),
  noteSave: (id: string | null, input: { title: string; body: string; project_id: string | null; tags: string[]; paper_ids: string[]; from_ai_draft?: boolean }) => call<NoteDetail>('note_save', { id, input }),
  noteDelete: (id: string) => call<void>('note_delete', { id }),
  noteAiDraft: (paperId: string, focus: string, job: string) => call<{ title: string; body: string; citations: Citation[]; removed_markers: number; removed_references: string[]; paper_id: string; model: string }>('note_ai_draft', { paperId, focus, job }),
  projects: () => call<Project[]>('projects_list'),
  projectSave: (id: string | null, input: { name: string; description: string; status?: string }) => call<Project>('project_save', { id, input }),
  projectDelete: (id: string) => call<void>('project_delete', { id }),
  projectPapers: (id: string, add: string[], remove: string[]) => call<Project>('project_papers', { id, add, remove }),
  graph: () => call<{ nodes: GNode[]; edges: GEdge[] }>('graph_get'),
  graphSuggest: () => call<number>('graph_suggest'),
  graphLink: (srcType: string, srcId: string, dstType: string, dstId: string, label: string) => call<void>('graph_link', { srcType, srcId, dstType, dstId, label }),
  graphResolve: (linkId: string, accept: boolean) => call<void>('graph_resolve', { linkId, accept }),
  graphUnlink: (linkId: string) => call<void>('graph_unlink', { linkId }),
  reviewRun: (paperIds: string[], topic: string, job: string) => call<Review>('review_run', { paperIds, topic, job }),
  reviewList: () => call<Review[]>('review_list'),
  reviewDelete: (id: string) => call<void>('review_delete', { id }),
  exportFile: (target: ExportTarget, format: ExportFormat, path: string) => call<string>('export_file', { target, format, path }),
  exportObsidian: (dir: string) => call<{ folder: string; notes_written: number; papers_written: number; skipped_existing: number }>('export_obsidian', { dir }),
  backupCreate: (path: string, includeFiles: boolean, password: string | null) => call<any>('backup_create', { path, includeFiles, password }),
  backupInspect: (path: string, password: string | null) => call<any>('backup_inspect', { path, password }),
  backupRestore: (path: string, password: string | null, confirmReplace: boolean) => call<any>('backup_restore', { path, password, confirmReplace }),
  updateCheck: (channel: string) => call<any>('update_check', { channel }),
  updateInstall: (job: string) => call<void>('update_install', { job }),
  restart: () => call<void>('app_restart'),
};

/** 작업 이벤트 구독 (특정 job id만) */
export async function onJob(job: string, cb: (e: JobEvent) => void): Promise<UnlistenFn> {
  return listen<JobEvent>('job', (ev) => {
    if (ev.payload.id === job) cb(ev.payload);
  });
}
