import type { QueryKey } from '@tanstack/react-query';
import type { WorktreeChanged } from '../api';

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
