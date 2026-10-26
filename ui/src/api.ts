import { invoke } from '@tauri-apps/api/core';

export type FileStatus = 'added' | 'modified' | 'deleted' | 'untracked';
export type Profile = 'flutter' | 'node' | 'python' | 'default';
export type BaseSource = 'config' | 'pr' | 'origin';

/** none: 체크 안 함, viewed: 체크했고 그 뒤 안 바뀜, changed: 체크한 뒤 다시 바뀜 */
export type ViewedState = 'none' | 'viewed' | 'changed';

export interface ChangedFile {
  path: string;
  status: FileStatus;
  /** null이면 바이너리 (또는 1MB 넘는 untracked) */
  additions: number | null;
  deletions: number | null;
  /** 생성·lock 파일. 펼치기 전에는 diff를 계산하지 않는다 */
  collapsed: boolean;
  viewed: ViewedState;
  /** session·turn view에서 원본(기준 내용)을 알 수 없어 diff가 부정확할 수 있다 */
  uncertain: boolean;
}

export type PatchBody =
  /** oldSource: old 쪽 파일 전체 (하이라이팅용). old 쪽 파일이 없으면 null */
  | { kind: 'text'; diff: string; oldSource: string | null }
  | { kind: 'binary'; oldSize: number | null; newSize: number | null }
  | { kind: 'tooLarge'; oldSize: number | null; newSize: number | null };

export interface FilePatch {
  path: string;
  body: PatchBody;
  hash: string;
}

export interface WorktreeSummary {
  id: string;
  path: string;
  branch: string | null;
  base: string;
  baseSource: BaseSource;
  changedFiles: number;
  /** 변경 파일 중 봤음 체크가 안 됐거나 다시 바뀐 파일 수 */
  unviewedFiles: number;
  profile: Profile;
  /** Unix ms. 앱이 마지막으로 변경을 알린 시각 */
  lastChangedAt: number | null;
  /** 목록 계산이 느릴 때만 값이 있다 */
  slowListMs: number | null;
  fsmonitor: boolean;
  /** 이 worktree에 지금 활동 중인 Claude Code 세션이 있다 */
  activeSession: boolean;
  /** 아직 Claude에게 전달되지 않은 리뷰 코멘트 수 */
  unsentComments: number;
  /** 열지 못한 이유 */
  error: string | null;
}

export interface RepoSummary {
  id: string;
  name: string;
  profile: Profile;
  worktrees: WorktreeSummary[];
}

export interface EntryError {
  path: string;
  message: string;
}

export interface ReposResponse {
  repos: RepoSummary[];
  errors: EntryError[];
  /** 다음 prompt에 미전송 코멘트를 자동으로 붙인다 (꺼져 있으면 복사해서 붙여 넣는다) */
  autoAttachComments: boolean;
}

/** branch: 브랜치 전체, session: 이번 세션, turn: 마지막 응답 */
export type View = 'branch' | 'session' | 'turn';

export interface SessionSummary {
  id: string;
  /** Unix ms */
  startedAt: number;
  /** Unix ms. 목록은 이 값의 내림차순 */
  lastEventAt: number;
  active: boolean;
  /** "마지막 응답" view가 보여 주는 turn 번호 */
  turn: number;
  /** 그 turn이 아직 끝나지 않았다 */
  turnOpen: boolean;
  touchedFiles: number;
}

export interface SessionUpdated {
  sessionIds: string[];
  worktreeIds: string[];
}

export interface WorktreeChanged {
  worktreeId: string;
  changedPaths: string[];
  scope: 'file' | 'list' | 'base';
}

export const listRepos = () => invoke<ReposResponse>('list_repos');

export const listSessions = (worktreeId: string) =>
  invoke<SessionSummary[]>('list_sessions', { worktreeId });

/** `sessionId`는 branch view에서 null, session·turn view에서 필수 */
export const listFiles = (worktreeId: string, view: View, sessionId: string | null) =>
  invoke<ChangedFile[]>('list_files', { worktreeId, view, sessionId });

export const getPatch = (worktreeId: string, view: View, path: string, sessionId: string | null) =>
  invoke<FilePatch>('get_patch', { worktreeId, view, path, sessionId });

export const setViewed = (worktreeId: string, path: string, viewed: boolean, hash: string | null) =>
  invoke<void>('set_viewed', { worktreeId, path, viewed, hash });

/** old: 삭제된 줄(옛 줄 번호), new: 추가·문맥 줄(새 줄 번호) */
export type Side = 'old' | 'new';

export interface Comment {
  id: number;
  worktree: string;
  path: string;
  side: Side;
  line: number;
  /** 코멘트를 단 view. `old` side는 같은 view에서만 위치를 맞춘다 */
  view: View;
  /** 코멘트를 달 때 그 줄의 내용 (`+`/`-` 접두 없이). 위치 맞추기에 쓴다 */
  snippet: string;
  body: string;
  /** Unix ms */
  createdAt: number;
  updatedAt: number;
  /** Unix ms. null이면 미전송 */
  sentAt: number | null;
  /** 전송 대상. session id 또는 `copy` */
  sentTo: string | null;
  /** 첨부한 prompt가 연 turn. `copy`로 보냈으면 null */
  sentTurn: number | null;
  /** Unix ms. null이면 해결되지 않음. 전송된 코멘트만 해결될 수 있다 */
  resolvedAt: number | null;
}

/** 목록에 주는 코멘트: 저장된 값 + 그 코멘트를 받은 세션의 응답 turn (`sentTurn` 이후 그 파일을 건드린 가장 큰 turn) */
export interface CommentItem extends Comment {
  replyTurn: number | null;
}

export type NewComment = Pick<Comment, 'path' | 'side' | 'line' | 'view' | 'snippet' | 'body'>;

export interface CopiedComments {
  markdown: string;
  ids: number[];
}

export const listComments = (worktreeId: string) => invoke<CommentItem[]>('list_comments', { worktreeId });

export const addComment = (worktreeId: string, comment: NewComment) =>
  invoke<Comment>('add_comment', { worktreeId, ...comment });

export const updateComment = (id: number, body: string) => invoke<Comment>('update_comment', { id, body });

export const deleteComment = (id: number) => invoke<void>('delete_comment', { id });

/** 전송된 코멘트를 지운다. 지운 수를 돌려준다 */
export const clearSentComments = (worktreeId: string) => invoke<number>('clear_sent_comments', { worktreeId });

/** 미전송 코멘트를 markdown으로 만든다 (전송됨 표시는 `markCommentsSent`로 따로 한다) */
export const copyComments = (worktreeId: string) => invoke<CopiedComments>('copy_comments', { worktreeId });

export const markCommentsSent = (ids: number[]) => invoke<number>('mark_comments_sent', { ids });

/** 전송된 코멘트를 해결됨으로. 미전송이면 오류 */
export const resolveComment = (id: number) => invoke<Comment>('resolve_comment', { id });

export const unresolveComment = (id: number) => invoke<Comment>('unresolve_comment', { id });

/** 전송된 코멘트를 미전송으로 되돌린다 (다음 prompt에 다시 붙는다) */
export const resendComment = (id: number) => invoke<Comment>('resend_comment', { id });
