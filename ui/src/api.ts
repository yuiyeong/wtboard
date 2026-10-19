import { invoke } from '@tauri-apps/api/core';

export type FileStatus = 'added' | 'modified' | 'deleted' | 'untracked';
export type Profile = 'flutter' | 'node' | 'python' | 'default';
export type BaseSource = 'config' | 'pr' | 'origin';

/** none: 체크 안 함, viewed: 체크했고 그 뒤 안 바뀜, changed: 체크한 뒤 다시 바뀜 */
export type ViewedState = 'none' | 'viewed' | 'changed';

export interface ChangedFile {
  path: string;
  status: FileStatus;
  /** null이면 바이너리 (또는 1MB 넘는 untracked) */
  additions: number | null;
  deletions: number | null;
  /** 생성·lock 파일. 펼치기 전에는 diff를 계산하지 않는다 */
  collapsed: boolean;
  viewed: ViewedState;
}

export type PatchBody =
  | { kind: 'text'; diff: string }
  | { kind: 'binary'; oldSize: number | null; newSize: number | null }
  | { kind: 'tooLarge'; oldSize: number | null; newSize: number | null };

export interface FilePatch {
  path: string;
  body: PatchBody;
  hash: string;
}

export interface WorktreeSummary {
  id: string;
  path: string;
  branch: string | null;
  base: string;
  baseSource: BaseSource;
  changedFiles: number;
  /** 변경 파일 중 봤음 체크가 안 됐거나 다시 바뀐 파일 수 */
  unviewedFiles: number;
  profile: Profile;
  /** Unix ms. 앱이 마지막으로 변경을 알린 시각 */
  lastChangedAt: number | null;
  /** 목록 계산이 느릴 때만 값이 있다 */
  slowListMs: number | null;
  fsmonitor: boolean;
  /** 열지 못한 이유 */
  error: string | null;
}

export interface RepoSummary {
  id: string;
  name: string;
  profile: Profile;
  worktrees: WorktreeSummary[];
}

export interface EntryError {
  path: string;
  message: string;
}

export interface ReposResponse {
  repos: RepoSummary[];
  errors: EntryError[];
}

export type View = 'branch';

export interface WorktreeChanged {
  worktreeId: string;
  changedPaths: string[];
  scope: 'file' | 'list' | 'base';
}

export const listRepos = () => invoke<ReposResponse>('list_repos');

export const listFiles = (worktreeId: string, view: View) =>
  invoke<ChangedFile[]>('list_files', { worktreeId, view });

export const getPatch = (worktreeId: string, view: View, path: string) =>
  invoke<FilePatch>('get_patch', { worktreeId, view, path });

export const setViewed = (worktreeId: string, path: string, viewed: boolean, hash: string | null) =>
  invoke<void>('set_viewed', { worktreeId, path, viewed, hash });
