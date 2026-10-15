import { useQuery } from '@tanstack/react-query';
import { listFiles, listRepos, type WorktreeSummary } from './api';
import { FileDiff } from './components/FileDiff';
import { FileList } from './components/FileList';
import { useUiStore } from './store';
import { useWorktreeEvents } from './useWorktreeEvents';

export function App() {
  useWorktreeEvents();
  const repos = useQuery({ queryKey: ['repos'], queryFn: listRepos });
  if (repos.error) return <ErrorScreen message={String(repos.error)} />;
  const repo = repos.data?.[0];
  const worktree = repo?.worktrees[0];
  if (!repo || !worktree) return <p className="p-4 text-gray-500">불러오는 중…</p>;
  return <WorktreeView repoName={repo.name} worktree={worktree} />;
}

function WorktreeView({ repoName, worktree }: { repoName: string; worktree: WorktreeSummary }) {
  const { viewType, setViewType } = useUiStore();
  const files = useQuery({
    queryKey: ['files', worktree.id, 'branch'],
    queryFn: () => listFiles(worktree.id, 'branch'),
  });

  return (
    <div className="flex h-screen flex-col">
      <header className="flex items-center gap-3 border-b border-gray-200 px-4 py-2 text-sm">
        <strong>{repoName}</strong>
        <span className="font-mono">{worktree.branch ?? '(detached)'}</span>
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
          <FileList files={files.data ?? []} />
        </aside>
        <main className="flex-1 overflow-y-auto p-4">
          {files.error && <p className="text-red-600">{String(files.error)}</p>}
          {files.data?.length === 0 && <p className="text-gray-500">base 대비 변경 사항이 없습니다.</p>}
          {files.data?.map((f) => (
            <FileDiff key={f.path} worktreeId={worktree.id} file={f} viewType={viewType} />
          ))}
        </main>
      </div>
    </div>
  );
}

function ErrorScreen({ message }: { message: string }) {
  return (
    <div className="p-6">
      <h1 className="mb-2 font-bold">wtboard를 시작할 수 없습니다</h1>
      <pre className="whitespace-pre-wrap text-sm text-red-600">{message}</pre>
    </div>
  );
}
