import type { RepoSummary, WorktreeSummary } from '../api';

export interface Selection {
  repoId: string;
  worktreeId: string;
}

interface Current {
  repo: RepoSummary;
  worktree: WorktreeSummary;
}

/**
 * 저장된 선택을 지금 목록에 맞춘다. worktree가 사라졌으면 같은 레포에서, 레포도 없으면 첫 레포에서 고른다.
 * 직접 고른 worktree는 열기 오류가 있어도 그대로 두고, 대신 고를 때는 오류 없는 worktree를 먼저 고른다.
 */
export function resolveSelection(repos: RepoSummary[], selection: Selection | null): Current | null {
  const usable = repos.filter((r) => r.worktrees.length > 0);
  const repo = usable.find((r) => r.id === selection?.repoId);
  const chosen = repo?.worktrees.find((w) => w.id === selection?.worktreeId);
  if (repo && chosen) return { repo, worktree: chosen };
  const fallback = repo ?? usable.find((r) => r.worktrees.some((w) => !w.error)) ?? usable[0];
  if (!fallback) return null;
  return { repo: fallback, worktree: fallback.worktrees.find((w) => !w.error) ?? fallback.worktrees[0] };
}

/** worktree id가 속한 레포를 찾아 선택으로 만든다. 목록에 없으면 null (tray의 `select-worktree`) */
export function selectionForWorktree(repos: RepoSummary[], worktreeId: string): Selection | null {
  const repo = repos.find((r) => r.worktrees.some((w) => w.id === worktreeId));
  return repo ? { repoId: repo.id, worktreeId } : null;
}

/** sidebar 표시 이름. detached HEAD면 폴더 이름 */
export function worktreeLabel(w: WorktreeSummary): string {
  return w.branch ?? w.path.split('/').filter(Boolean).pop() ?? w.path;
}
