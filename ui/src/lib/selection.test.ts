import { describe, expect, it } from 'vitest';
import type { RepoSummary, WorktreeSummary } from '../api';
import { worktreeFixture } from './fixtures';
import { resolveSelection, selectionForWorktree, worktreeLabel } from './selection';

const wt = (id: string, branch: string | null = id) => worktreeFixture({ id, path: `/dev/${id}`, branch });
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

  it('자동으로 고를 때는 오류 없는 worktree를 먼저 고르고, 직접 고른 것은 오류가 있어도 그대로 둔다', () => {
    const broken = { ...wt('x1'), error: 'base 없음' };
    const list = [repo('x', broken, wt('x2')), repo('y', wt('y1'))];
    expect(resolveSelection(list, null)?.worktree.id).toBe('x2');
    expect(resolveSelection(list, { repoId: 'x', worktreeId: 'gone' })?.worktree.id).toBe('x2');
    expect(resolveSelection(list, { repoId: 'x', worktreeId: 'x1' })?.worktree.id).toBe('x1');
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

describe('selectionForWorktree', () => {
  it('있는 id는 {repoId, worktreeId}', () => {
    expect(selectionForWorktree(repos, 'b1')).toEqual({ repoId: 'b', worktreeId: 'b1' });
    expect(selectionForWorktree(repos, 'a2')).toEqual({ repoId: 'a', worktreeId: 'a2' });
  });

  it('없는 id는 null', () => {
    expect(selectionForWorktree(repos, 'gone')).toBeNull();
  });
});
