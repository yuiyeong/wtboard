import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { listFiles, type RepoSummary, type WorktreeSummary } from '../api';
import { worktreeLabel } from '../lib/selection';
import { buildTree, flattenFiles, splitCollapsed } from '../lib/tree';
import { useUiStore } from '../store';
import { FileDiff } from './FileDiff';
import { CollapsedGroup, FileTree } from './FileTree';

export function WorktreeView({ repo, worktree }: { repo: RepoSummary; worktree: WorktreeSummary }) {
  const viewType = useUiStore((s) => s.viewType);
  const setViewType = useUiStore((s) => s.setViewType);
  const files = useQuery({
    queryKey: ['files', worktree.id, 'branch'],
    queryFn: () => listFiles(worktree.id, 'branch'),
  });
  const { visible, collapsed } = useMemo(() => splitCollapsed(files.data ?? []), [files.data]);
  const tree = useMemo(() => buildTree(visible), [visible]);
  const collapsedTree = useMemo(() => buildTree(collapsed), [collapsed]);
  // diff 순서를 트리와 맞춘다. 접힌 파일은 맨 뒤
  const ordered = useMemo(() => [...flattenFiles(tree), ...flattenFiles(collapsedTree)], [tree, collapsedTree]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="flex items-center gap-3 border-b border-gray-200 px-4 py-2 text-sm">
        <strong>{repo.name}</strong>
        <span className="font-mono">{worktreeLabel(worktree)}</span>
        <span className="text-gray-500">← {worktree.base}</span>
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
      </header>
      <div className="flex min-h-0 flex-1">
        <aside className="w-80 shrink-0 overflow-y-auto border-r border-gray-200">
          <p className="px-3 py-2 text-xs text-gray-500">변경 파일 {files.data?.length ?? 0}개</p>
          <FileTree nodes={tree} />
          {collapsed.length > 0 && <CollapsedGroup count={collapsed.length} nodes={collapsedTree} />}
        </aside>
        {/* 위쪽 padding을 두면 sticky 파일 헤더가 그만큼 아래에 멈추므로 spacer로 띄운다 */}
        <main className="flex-1 overflow-y-auto px-4 pb-4">
          <div className="h-4" />
          {files.error && <p className="text-red-600">{String(files.error)}</p>}
          {files.data?.length === 0 && <p className="text-gray-500">base 대비 변경 사항이 없습니다.</p>}
          {ordered.map((f) => (
            <FileDiff key={f.path} worktreeId={worktree.id} file={f} viewType={viewType} />
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
