import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { listFiles, type RepoSummary, type WorktreeSummary } from '../api';
import { worktreeLabel } from '../lib/selection';
import { buildTree, flattenFiles, splitCollapsed, type TreeNode } from '../lib/tree';
import { viewedProgress } from '../lib/viewed';
import { useUiStore } from '../store';
import { useSetViewed } from '../useSetViewed';
import { FileDiff } from './FileDiff';
import { FileTree } from './FileTree';
import { FsmonitorBanner } from './FsmonitorBanner';

/** 생성·lock 파일을 모아 두는 가짜 디렉터리. 실제 경로와 겹치지 않게 NUL로 시작한다 */
const COLLAPSED_DIR = '\0collapsed';

/** 접힌 묶음 안의 폴더가 위쪽 같은 이름 폴더와 접기 상태를 공유하지 않게 path를 구분한다 */
function underCollapsedDir(nodes: TreeNode[]): TreeNode[] {
  return nodes.map((n) =>
    n.kind === 'dir' ? { ...n, path: `${COLLAPSED_DIR}/${n.path}`, children: underCollapsedDir(n.children) } : n,
  );
}

export function WorktreeView({ repo, worktree }: { repo: RepoSummary; worktree: WorktreeSummary }) {
  const viewType = useUiStore((s) => s.viewType);
  const setViewType = useUiStore((s) => s.setViewType);
  const files = useQuery({
    queryKey: ['files', worktree.id, 'branch'],
    queryFn: () => listFiles(worktree.id, 'branch'),
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
        <span className={`tabular-nums ${allViewed ? 'text-green-600' : 'text-gray-500'}`}>
          확인 {progress.viewed}/{progress.total}
        </span>
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
      </header>
      <FsmonitorBanner repoId={repo.id} worktree={worktree} />
      <div className="flex min-h-0 flex-1">
        <aside className="w-80 shrink-0 overflow-y-auto border-r border-gray-200">
          <p className="px-3 py-2 text-xs text-gray-500">변경 파일 {files.data?.length ?? 0}개</p>
          <FileTree nodes={tree} initiallyCollapsed={[COLLAPSED_DIR]} />
        </aside>
        {/* 위쪽 padding을 두면 sticky 파일 헤더가 그만큼 아래에 멈추므로 spacer로 띄운다 */}
        <main className="flex-1 overflow-y-auto px-4 pb-4">
          <div className="h-4" />
          {files.error && <p className="text-red-600">{String(files.error)}</p>}
          {files.data?.length === 0 && <p className="text-gray-500">base 대비 변경 사항이 없습니다.</p>}
          {ordered.map((f) => (
            <FileDiff key={f.path} worktreeId={worktree.id} file={f} viewType={viewType} onSetViewed={onSetViewed} />
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
