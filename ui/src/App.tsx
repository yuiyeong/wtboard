import { useQuery } from '@tanstack/react-query';
import { listRepos } from './api';
import { Sidebar } from './components/Sidebar';
import { WorktreeError, WorktreeView } from './components/WorktreeView';
import { resolveSelection } from './lib/selection';
import { useUiStore } from './store';
import { useWorktreeEvents } from './useWorktreeEvents';
import { useZoom } from './useZoom';

export function App() {
  useWorktreeEvents();
  useZoom();
  const selection = useUiStore((s) => s.selection);
  const select = useUiStore((s) => s.select);
  // 앱이 레포를 여는 동안 list_repos는 기다렸다가 답한다.
  // SessionEnd 없이 죽은 세션은 30분 뒤 비활성이 되는데 그것을 알리는 event가 없으므로 주기적으로 다시 읽는다 (●)
  const repos = useQuery({ queryKey: ['repos'], queryFn: listRepos, refetchInterval: 60_000 });
  if (repos.error) return <ErrorScreen message={String(repos.error)} />;
  if (!repos.data) return <p className="p-4 text-gray-400">불러오는 중…</p>;

  const current = resolveSelection(repos.data.repos, selection);
  return (
    <div className="flex h-screen">
      <Sidebar
        repos={repos.data.repos}
        errors={repos.data.errors}
        selectedId={current?.worktree.id ?? null}
        onSelect={select}
      />
      {!current ? (
        <p className="p-4 text-gray-400">열 수 있는 worktree가 없습니다. 왼쪽 아래 오류를 확인하세요.</p>
      ) : current.worktree.error ? (
        <WorktreeError worktree={current.worktree} />
      ) : (
        <WorktreeView
          key={current.worktree.id}
          repo={current.repo}
          worktree={current.worktree}
          autoAttachComments={repos.data.autoAttachComments}
        />
      )}
    </div>
  );
}

function ErrorScreen({ message }: { message: string }) {
  return (
    <div className="p-6">
      <h1 className="mb-2 font-bold">wtboard를 시작할 수 없습니다</h1>
      <pre className="whitespace-pre-wrap text-sm text-red-400">{message}</pre>
    </div>
  );
}
