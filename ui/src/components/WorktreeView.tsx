import { useCallback, useEffect, useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import {
  listFiles,
  listSessions,
  type CommentItem,
  type RepoSummary,
  type SessionSummary,
  type View,
  type WorktreeSummary,
} from '../api';
import { scrollToFile } from '../lib/anchor';
import { byPath, openComments, replyTarget, unsentByPath, type ReplyTarget } from '../lib/comments';
import { worktreeLabel } from '../lib/selection';
import { effectiveView, filesKey, pickSession } from '../lib/sessions';
import { buildTree, flattenFiles, splitCollapsed, type TreeNode } from '../lib/tree';
import { viewedProgress } from '../lib/viewed';
import { requestScroll, takeScroll, useNavStore } from '../nav';
import { useUiStore } from '../store';
import { useCommentActions, useComments } from '../useComments';
import { useSetViewed } from '../useSetViewed';
import { CommentsPopover } from './CommentsPopover';
import { FileDiff } from './FileDiff';
import { FileTree } from './FileTree';
import { FsmonitorBanner } from './FsmonitorBanner';
import { ViewTabs } from './ViewTabs';

/** 생성·lock 파일을 모아 두는 가짜 디렉터리. 실제 경로와 겹치지 않게 NUL로 시작한다 */
const COLLAPSED_DIR = '\0collapsed';

/** 코멘트가 없는 파일에 넘기는 빈 배열. 매번 새 배열을 만들면 FileDiff memo가 깨진다 */
const NO_COMMENTS: CommentItem[] = [];
const NO_SESSIONS: SessionSummary[] = [];

const EMPTY_TEXT: Record<View, string> = {
  branch: 'base 대비 변경 사항이 없습니다.',
  session: '이 세션이 바꾼 파일이 없습니다.',
  turn: '이 응답이 바꾼 파일이 없습니다.',
};

/** 접힌 묶음 안의 폴더가 위쪽 같은 이름 폴더와 접기 상태를 공유하지 않게 path를 구분한다 */
function underCollapsedDir(nodes: TreeNode[]): TreeNode[] {
  return nodes.map((n) =>
    n.kind === 'dir' ? { ...n, path: `${COLLAPSED_DIR}/${n.path}`, children: underCollapsedDir(n.children) } : n,
  );
}

interface Props {
  repo: RepoSummary;
  worktree: WorktreeSummary;
  /** config의 자동 첨부. 꺼져 있으면 코멘트 목록에 복사 안내를 보인다 */
  autoAttachComments: boolean;
}

export function WorktreeView({ repo, worktree, autoAttachComments }: Props) {
  const viewType = useUiStore((s) => s.viewType);
  const setViewType = useUiStore((s) => s.setViewType);
  const chosenView = useUiStore((s) => s.views[worktree.id] ?? 'branch');
  const setView = useUiStore((s) => s.setView);
  const rememberedSession = useUiStore((s) => s.sessions[worktree.id]);
  const selectSession = useUiStore((s) => s.selectSession);
  const sessions = useQuery({ queryKey: ['sessions', worktree.id], queryFn: () => listSessions(worktree.id) });
  const session = pickSession(sessions.data ?? [], rememberedSession);
  const view = effectiveView(chosenView, session);
  const sessionId = view === 'branch' || !session ? null : session.id;
  const files = useQuery({
    queryKey: filesKey(worktree.id, view, sessionId),
    queryFn: () => listFiles(worktree.id, view, sessionId),
  });
  const { onSetViewed, error: viewedError } = useSetViewed(worktree.id);
  const progress = useMemo(() => viewedProgress(files.data ?? []), [files.data]);
  const allViewed = progress.total > 0 && progress.viewed === progress.total;
  const { tree, ordered } = useMemo(() => {
    const { visible, collapsed } = splitCollapsed(files.data ?? []);
    const tree: TreeNode[] = buildTree(visible);
    if (collapsed.length > 0) {
      const name = `접힌 파일 (${collapsed.length})`;
      tree.push({ kind: 'dir', name, path: COLLAPSED_DIR, children: underCollapsedDir(buildTree(collapsed)) });
    }
    return { tree, ordered: flattenFiles(tree) }; // diff 순서를 트리와 맞춘다. 접힌 파일은 맨 뒤
  }, [files.data]);
  const listedPaths = useMemo(() => new Set(files.data?.map((f) => f.path)), [files.data]);

  const comments = useComments(worktree.id);
  const commentList = comments.data ?? NO_COMMENTS;
  // 해결됨 코멘트는 diff에 그리지 않는다 (목록 popover에만)
  const commentsByPath = useMemo(() => byPath(openComments(commentList)), [commentList]);
  const unsentPaths = useMemo(() => unsentByPath(commentList), [commentList]);
  // mutateAsync는 참조가 안정적이라 FileDiff memo를 깨지 않는다
  const { add, update, remove, resolve, resend } = useCommentActions(worktree.id);
  const sessionList = sessions.data ?? NO_SESSIONS;
  const replyTargetOf = useCallback((c: CommentItem) => replyTarget(c, sessionList), [sessionList]);
  // `응답 #n 보기`: 그 세션의 응답 view로 옮기고, 파일 목록이 오면 그 파일로 스크롤한다
  const openReply = useCallback(
    (c: CommentItem, target: ReplyTarget) => {
      selectSession(worktree.id, target.sessionId);
      setView(worktree.id, target.view);
      requestScroll(worktree.id, c.path);
    },
    [worktree.id, selectSession, setView],
  );
  const pendingScroll = useNavStore((s) => s.pendingScroll);
  useEffect(() => {
    const path = takeScroll(worktree.id, listedPaths);
    if (path) scrollToFile(path);
  }, [pendingScroll, listedPaths, worktree.id]);

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <header className="flex items-center gap-3 border-b border-gray-200 px-4 py-2 text-sm">
        <strong>{repo.name}</strong>
        <span className="font-mono">{worktreeLabel(worktree)}</span>
        <span className="text-gray-500">
          ← {worktree.base}
          {worktree.baseSource === 'pr' && ' (PR)'}
        </span>
        <span className="flex-1" />
        {viewedError && (
          <span className="min-w-0 truncate text-xs text-red-600" title={viewedError}>
            {viewedError}
          </span>
        )}
        {view === 'branch' && (
          <span className={`tabular-nums ${allViewed ? 'text-green-600' : 'text-gray-500'}`}>
            확인 {progress.viewed}/{progress.total}
          </span>
        )}
        <CommentsPopover
          worktreeId={worktree.id}
          comments={commentList}
          listedPaths={listedPaths}
          autoAttach={autoAttachComments}
        />
      </header>
      <div className="flex items-center gap-3 border-b border-gray-200 px-4 py-1.5 text-sm">
        <ViewTabs
          view={view}
          sessions={sessions.data ?? []}
          session={session}
          onView={(v) => setView(worktree.id, v)}
          onSession={(id) => selectSession(worktree.id, id)}
        />
        <span className="flex-1" />
        <div className="flex overflow-hidden rounded border border-gray-300">
          {(['split', 'unified'] as const).map((v) => (
            <button
              key={v}
              onClick={() => setViewType(v)}
              className={`px-2 py-1 ${viewType === v ? 'bg-gray-800 text-white' : ''}`}
            >
              {v}
            </button>
          ))}
        </div>
      </div>
      <FsmonitorBanner repoId={repo.id} worktree={worktree} />
      <div className="flex min-h-0 flex-1">
        <aside className="w-80 shrink-0 overflow-y-auto border-r border-gray-200">
          <p className="px-3 py-2 text-xs text-gray-500">변경 파일 {files.data?.length ?? 0}개</p>
          <FileTree nodes={tree} initiallyCollapsed={[COLLAPSED_DIR]} unsent={unsentPaths} />
        </aside>
        {/* 위쪽 padding을 두면 sticky 파일 헤더가 그만큼 아래에 멈추므로 spacer로 띄운다 */}
        <main className="flex-1 overflow-y-auto px-4 pb-4">
          <div className="h-4" />
          {files.error && <p className="text-red-600">{String(files.error)}</p>}
          {files.data?.length === 0 && <p className="text-gray-500">{EMPTY_TEXT[view]}</p>}
          {/* view·세션마다 펼침 상태를 새로 시작한다 */}
          {ordered.map((f) => (
            <FileDiff
              key={`${view}:${sessionId}:${f.path}`}
              worktreeId={worktree.id}
              file={f}
              view={view}
              sessionId={sessionId}
              viewType={viewType}
              onSetViewed={onSetViewed}
              comments={commentsByPath.get(f.path) ?? NO_COMMENTS}
              onAdd={add.mutateAsync}
              onUpdate={update.mutateAsync}
              onDelete={remove.mutateAsync}
              onResolve={resolve.mutateAsync}
              onResend={resend.mutateAsync}
              replyTargetOf={replyTargetOf}
              onOpenReply={openReply}
            />
          ))}
        </main>
      </div>
    </div>
  );
}

export function WorktreeError({ worktree }: { worktree: WorktreeSummary }) {
  return (
    <div className="p-6">
      <h1 className="mb-1 font-bold">{worktreeLabel(worktree)}를 열 수 없습니다</h1>
      <p className="mb-3 font-mono text-xs text-gray-500">{worktree.path}</p>
      <pre className="text-sm whitespace-pre-wrap text-red-600">{worktree.error}</pre>
    </div>
  );
}
