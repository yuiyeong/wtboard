import { create } from 'zustand';
import type { WorktreeSummary } from '../api';
import { FSMONITOR_COMMANDS, showFsmonitorBanner } from '../lib/banner';

/** banner를 닫은 레포 id. 앱 실행 중에만 기억한다 (저장하지 않음) */
const useDismissed = create<{ repos: string[]; dismiss: (repoId: string) => void }>((set) => ({
  repos: [],
  dismiss: (repoId) => set((s) => (s.repos.includes(repoId) ? s : { repos: [...s.repos, repoId] })),
}));

export function FsmonitorBanner({ repoId, worktree }: { repoId: string; worktree: WorktreeSummary }) {
  const dismissed = useDismissed((s) => s.repos.includes(repoId));
  const dismiss = useDismissed((s) => s.dismiss);
  if (!showFsmonitorBanner(worktree, dismissed)) return null;
  return (
    <div className="flex items-start gap-3 border-b border-amber-800 bg-amber-950 px-4 py-2 text-sm">
      <div className="flex-1">
        <p className="mb-1">
          이 레포는 변경 목록 계산에 {worktree.slowListMs}ms 걸립니다. 레포 폴더에서 다음을 실행하면 빨라집니다.
        </p>
        {FSMONITOR_COMMANDS.map((cmd) => (
          <div key={cmd} className="flex items-center gap-2">
            <code className="font-mono text-xs">{cmd}</code>
            <button className="text-xs text-blue-400 hover:underline" onClick={() => navigator.clipboard.writeText(cmd)}>
              복사
            </button>
          </div>
        ))}
      </div>
      <button className="text-xs text-gray-400 hover:underline" onClick={() => dismiss(repoId)}>
        닫기
      </button>
    </div>
  );
}
