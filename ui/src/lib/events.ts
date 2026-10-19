import type { InvalidateQueryFilters, QueryKey } from '@tanstack/react-query';
import type { SessionUpdated, WorktreeChanged } from '../api';

/** event 하나로 무효화할 query key prefix 목록 */
export function keysToInvalidate(ev: WorktreeChanged): QueryKey[] {
  const keys: QueryKey[] = [['repos'], ['files', ev.worktreeId]];
  if (ev.scope === 'base') {
    keys.push(['patch', ev.worktreeId]);
  } else {
    for (const path of ev.changedPaths) keys.push(['patch', ev.worktreeId, path]);
  }
  return keys;
}

/**
 * session-updated event 하나로 무효화할 query들.
 * patch는 그 worktree의 session·turn view patch 전체를 무효화한다. 새 prompt로 "마지막 응답"의 대상 turn이 바뀌면
 * 같은 path·view·session key의 내용이 달라지기 때문이다 (화면에 보이는 patch만 다시 가져온다).
 * branch view patch는 세션 기록과 무관하다 (`patchKey`의 4번째가 view)
 */
export function filtersForSessionUpdated(ev: SessionUpdated): InvalidateQueryFilters[] {
  const filters: InvalidateQueryFilters[] = [{ queryKey: ['repos'] }];
  for (const id of ev.worktreeIds) {
    filters.push(
      { queryKey: ['sessions', id] },
      { queryKey: ['files', id, 'session'] },
      { queryKey: ['files', id, 'turn'] },
      { queryKey: ['patch', id], predicate: (q) => q.queryKey[3] === 'session' || q.queryKey[3] === 'turn' },
    );
  }
  return filters;
}
