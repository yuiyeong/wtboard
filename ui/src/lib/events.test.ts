import { describe, expect, it } from 'vitest';
import { keysForSessionUpdated, keysToInvalidate } from './events';

describe('keysToInvalidate', () => {
  it('file scope는 바뀐 파일의 patch만 무효화한다', () => {
    expect(keysToInvalidate({ worktreeId: 'w', scope: 'file', changedPaths: ['a.py', 'b.py'] })).toEqual([
      ['repos'],
      ['files', 'w'],
      ['patch', 'w', 'a.py'],
      ['patch', 'w', 'b.py'],
    ]);
  });

  it('base scope는 그 worktree의 patch 전체를 무효화한다', () => {
    expect(keysToInvalidate({ worktreeId: 'w', scope: 'base', changedPaths: [] })).toEqual([
      ['repos'],
      ['files', 'w'],
      ['patch', 'w'],
    ]);
  });
});

describe('keysForSessionUpdated', () => {
  it('repos는 한 번, worktree마다 session·turn files와 patch 전체를 무효화한다', () => {
    expect(keysForSessionUpdated({ sessionIds: ['s1', 's2'], worktreeIds: ['w1', 'w2'] })).toEqual([
      ['repos'],
      ['sessions', 'w1'],
      ['files', 'w1', 'session'],
      ['files', 'w1', 'turn'],
      ['patch', 'w1'],
      ['sessions', 'w2'],
      ['files', 'w2', 'session'],
      ['files', 'w2', 'turn'],
      ['patch', 'w2'],
    ]);
  });
});
