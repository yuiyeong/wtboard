import { describe, expect, it } from 'vitest';
import { keysToInvalidate } from './events';

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
