import { QueryClient, type InvalidateQueryFilters, type QueryKey } from '@tanstack/react-query';
import { describe, expect, it } from 'vitest';
import type { WorktreeChanged } from '../api';
import { filtersForSessionUpdated, keysToInvalidate } from './events';
import { filesKey, patchKey } from './sessions';

/** `keys`의 query를 만들고 `filters`로 무효화한 뒤 무효화된 것만 돌려준다 (실제 TanStack prefix 매칭) */
function invalidated(keys: QueryKey[], filters: InvalidateQueryFilters[]): QueryKey[] {
  const client = new QueryClient();
  for (const key of keys) client.setQueryData(key, 1);
  for (const f of filters) void client.invalidateQueries(f);
  return keys.filter((key) => client.getQueryState(key)?.isInvalidated);
}

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

  it('prefix가 filesKey·patchKey의 실제 key와 맞는다 (모든 view)', () => {
    const keys: QueryKey[] = [
      filesKey('w', 'branch', null),
      filesKey('w', 'session', 's1'),
      filesKey('other', 'branch', null),
      patchKey('w', 'a.py', 'branch', null),
      patchKey('w', 'a.py', 'turn', 's1'),
      patchKey('w', 'c.py', 'branch', null),
    ];
    const ev: WorktreeChanged = { worktreeId: 'w', scope: 'file', changedPaths: ['a.py'] };
    expect(invalidated(keys, keysToInvalidate(ev).map((queryKey) => ({ queryKey })))).toEqual([
      filesKey('w', 'branch', null),
      filesKey('w', 'session', 's1'),
      patchKey('w', 'a.py', 'branch', null),
      patchKey('w', 'a.py', 'turn', 's1'),
    ]);
  });
});

describe('filtersForSessionUpdated', () => {
  it('worktree마다 sessions, session·turn의 files와 patch만 무효화한다. branch view는 그대로', () => {
    const keys: QueryKey[] = [
      ['repos'],
      ['sessions', 'w1'],
      ['sessions', 'w3'],
      filesKey('w1', 'branch', null),
      filesKey('w1', 'session', 's1'),
      filesKey('w2', 'turn', 's2'),
      filesKey('w3', 'session', 's1'),
      patchKey('w1', 'a.py', 'branch', null),
      patchKey('w1', 'a.py', 'session', 's1'),
      patchKey('w2', 'b.py', 'turn', 's2'),
      patchKey('w3', 'a.py', 'turn', 's1'),
    ];
    const filters = filtersForSessionUpdated({ sessionIds: ['s1', 's2'], worktreeIds: ['w1', 'w2'] });
    expect(invalidated(keys, filters)).toEqual([
      ['repos'],
      ['sessions', 'w1'],
      filesKey('w1', 'session', 's1'),
      filesKey('w2', 'turn', 's2'),
      patchKey('w1', 'a.py', 'session', 's1'),
      patchKey('w2', 'b.py', 'turn', 's2'),
    ]);
  });
});
