import type { ChangedFile, ViewedState, WorktreeSummary } from '../api';

/** 봤음으로 체크한 파일 수와 전체 파일 수 */
export function viewedProgress(files: ChangedFile[]): { viewed: number; total: number } {
  return { viewed: files.filter((f) => f.viewed === 'viewed').length, total: files.length };
}

/** 처음 보일 때 펼칠지. 본 파일과 collapsed 파일은 접는다 */
export function initiallyOpen(file: ChangedFile): boolean {
  return !file.collapsed && file.viewed !== 'viewed';
}

/** 봤음 상태가 바뀌었을 때의 펼침 여부. 체크하면 접고, 해제하거나 다시 바뀌면 펼친다. 상태가 그대로면 현재 값을 유지한다 */
export function nextOpen(prev: ViewedState, next: ViewedState, open: boolean): boolean {
  if (prev === next) return open;
  return next !== 'viewed';
}

/** optimistic update용. 대상 파일만 새 객체로 바꿔 나머지는 같은 참조를 유지한다 */
export function withViewed(files: ChangedFile[], path: string, viewed: boolean): ChangedFile[] {
  return files.map((f) => (f.path === path ? { ...f, viewed: viewed ? 'viewed' : 'none' } : f));
}

/** worktree 목록의 파일 수 표시. 안 본 파일이 있으면 `안 본/전체` */
export function countLabel(w: WorktreeSummary): string {
  return w.unviewedFiles > 0 ? `${w.unviewedFiles}/${w.changedFiles}` : `${w.changedFiles}`;
}

/** 봤음 저장 실패 표시. Tauri command는 문자열로 reject한다 */
export function viewedErrorText(error: unknown): string {
  return `봤음 저장 실패: ${error instanceof Error ? error.message : String(error)}`;
}
