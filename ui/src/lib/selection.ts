import type { RepoSummary, WorktreeSummary } from '../api';

export interface Selection {
  repoId: string;
  worktreeId: string;
}

export interface Current {
  repo: RepoSummary;
  worktree: WorktreeSummary;
}

/** 저장된 선택을 지금 목록에 맞춘다. worktree가 사라졌으면 같은 레포의 첫 worktree, 레포도 없으면 첫 레포의 첫 worktree */
export function resolveSelection(repos: RepoSummary[], selection: Selection | null): Current | null {
  const usable = repos.filter((r) => r.worktrees.length > 0);
  const repo = usable.find((r) => r.id === selection?.repoId);
  if (repo) {
    return { repo, worktree: repo.worktrees.find((w) => w.id === selection?.worktreeId) ?? repo.worktrees[0] };
  }
  const first = usable[0];
  return first ? { repo: first, worktree: first.worktrees[0] } : null;
}

/** sidebar 표시 이름. detached HEAD면 폴더 이름 */
export function worktreeLabel(w: WorktreeSummary): string {
  return w.branch ?? w.path.split('/').filter(Boolean).pop() ?? w.path;
}
