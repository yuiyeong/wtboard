import type { ChangedFile, CommentItem, WorktreeSummary } from '../api';

/** test용 WorktreeSummary. 필요한 필드만 덮어쓴다 */
export function worktreeFixture(overrides: Partial<WorktreeSummary> = {}): WorktreeSummary {
  return {
    id: 'w',
    path: '/w',
    branch: 'main',
    base: 'origin/main',
    baseSource: 'origin',
    changedFiles: 0,
    unviewedFiles: 0,
    profile: 'default',
    lastChangedAt: null,
    slowListMs: null,
    fsmonitor: false,
    activeSession: false,
    unsentComments: 0,
    error: null,
    ...overrides,
  };
}

/** test용 ChangedFile. 필요한 필드만 덮어쓴다 */
export function fileFixture(overrides: Partial<ChangedFile> = {}): ChangedFile {
  return {
    path: 'a.ts',
    status: 'modified',
    additions: 1,
    deletions: 0,
    collapsed: false,
    viewed: 'none',
    uncertain: false,
    ...overrides,
  };
}

/** test용 CommentItem. 필요한 필드만 덮어쓴다 */
export function commentFixture(overrides: Partial<CommentItem> = {}): CommentItem {
  return {
    id: 1,
    worktree: 'w',
    path: 'a.ts',
    side: 'new',
    line: 1,
    view: 'branch',
    snippet: 'x',
    body: '고쳐 주세요',
    createdAt: 0,
    updatedAt: 0,
    sentAt: null,
    sentTo: null,
    sentTurn: null,
    resolvedAt: null,
    replyTurn: null,
    ...overrides,
  };
}
