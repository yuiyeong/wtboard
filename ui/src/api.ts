import { invoke } from '@tauri-apps/api/core';

export type FileStatus = 'added' | 'modified' | 'deleted' | 'untracked';

export interface ChangedFile {
  path: string;
  status: FileStatus;
  /** null이면 바이너리 (또는 1MB 넘는 untracked) */
  additions: number | null;
  deletions: number | null;
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
  changedFiles: number;
}

export interface RepoSummary {
  name: string;
  worktrees: WorktreeSummary[];
}

export type View = 'branch';

export interface WorktreeChanged {
  worktreeId: string;
  changedPaths: string[];
  scope: 'file' | 'list' | 'base';
}

export const listRepos = () => invoke<RepoSummary[]>('list_repos');

export const listFiles = (worktreeId: string, view: View) =>
  invoke<ChangedFile[]>('list_files', { worktreeId, view });

export const getPatch = (worktreeId: string, view: View, path: string) =>
  invoke<FilePatch>('get_patch', { worktreeId, view, path });
