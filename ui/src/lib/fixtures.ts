import type { WorktreeSummary } from '../api';

/** test용 WorktreeSummary. 필요한 필드만 덮어쓴다 */
export function worktreeFixture(overrides: Partial<WorktreeSummary> = {}): WorktreeSummary {
  return {
    id: 'w',
    path: '/w',
    branch: 'main',
    base: 'origin/main',
    baseSource: 'origin',
    changedFiles: 0,
    profile: 'default',
    lastChangedAt: null,
    slowListMs: null,
    fsmonitor: false,
    error: null,
    ...overrides,
  };
}
