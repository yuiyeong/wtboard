import { describe, expect, it } from 'vitest';
import type { RepoSummary, WorktreeSummary } from '../api';
import { resolveSelection, worktreeLabel } from './selection';

const wt = (id: string, branch: string | null = id): WorktreeSummary => ({
  id,
  path: `/dev/${id}`,
  branch,
  base: 'origin/main',
  baseSource: 'origin',
  changedFiles: 0,
  profile: 'default',
  lastChangedAt: null,
  slowListMs: null,
  fsmonitor: false,
  error: null,
});
const repo = (id: string, ...worktrees: WorktreeSummary[]): RepoSummary => ({ id, name: id, profile: 'default', worktrees });
const repos = [repo('a', wt('a1'), wt('a2')), repo('b', wt('b1'))];

describe('resolveSelection', () => {
  it('저장된 선택이 목록에 있으면 그대로 쓴다', () => {
    expect(resolveSelection(repos, { repoId: 'b', worktreeId: 'b1' })?.worktree.id).toBe('b1');
  });

  it('선택한 worktree가 사라지면 같은 레포의 첫 worktree', () => {
    expect(resolveSelection(repos, { repoId: 'a', worktreeId: 'gone' })?.worktree.id).toBe('a1');
  });

  it('레포도 사라지면 첫 레포의 첫 worktree', () => {
    expect(resolveSelection(repos, { repoId: 'z', worktreeId: 'z1' })?.worktree.id).toBe('a1');
  });

  it('worktree가 없는 레포는 건너뛴다', () => {
    expect(resolveSelection([repo('empty'), repo('b', wt('b1'))], null)?.repo.id).toBe('b');
  });

  it('열 수 있는 것이 없으면 null', () => {
    expect(resolveSelection([], null)).toBeNull();
  });
});

describe('worktreeLabel', () => {
  it('브랜치 이름을 쓴다', () => {
    expect(worktreeLabel(wt('a1', 'feat/login'))).toBe('feat/login');
  });

  // Review Focus 5
  it('detached HEAD면 폴더 이름', () => {
    expect(worktreeLabel(wt('my-wt', null))).toBe('my-wt');
  });
});
