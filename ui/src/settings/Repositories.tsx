import { useEffect, useState } from 'react';
import { useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  addRepo,
  listBaseCandidates,
  listRepoEntries,
  removeRepo,
  setRepoBase,
  type RepoEntries,
  type RepoEntry,
} from '../api';
import { autoLabel, baseOptions } from '../lib/settings';

const ENTRIES_KEY = ['repo-entries'];
/** 줄마다 `[...CANDIDATES_KEY, path]` */
const CANDIDATES_KEY = ['base-candidates'];
const BUTTON = 'rounded border border-gray-300 dark:border-gray-700 px-2 py-0.5 hover:bg-gray-100 dark:hover:bg-gray-800 disabled:opacity-50';

/** command를 하나씩 돌리고, 실패하면 목록 아래에 그 문구를 보인다. 끝나면 목록과 base 후보를 다시 읽는다 */
type Run = (action: () => Promise<unknown>) => void;

/** 설정창의 Repositories: `config.toml`의 `[[repo]]` 목록, 추가·삭제·base 변경 */
export function Repositories() {
  const queryClient = useQueryClient();
  const entries = useQuery({ queryKey: ENTRIES_KEY, queryFn: listRepoEntries });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const refetch = () => refresh(queryClient);
    const unlistenUpdated = listen('worktrees-updated', refetch);
    // 닫기 = 숨기기라 다시 열어도 화면이 그대로다. 그 사이 손으로 고친 config를 보이게 앞에 올 때마다 다시 읽는다
    const unlistenFocus = getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (focused) refetch();
    });
    return () => {
      unlistenUpdated.then((off) => off());
      unlistenFocus.then((off) => off());
    };
  }, [queryClient]);

  const run: Run = (action) => {
    setBusy(true);
    action()
      .then(() => setError(null))
      .catch((e) => setError(String(e)))
      .finally(() => {
        setBusy(false);
        refresh(queryClient);
      });
  };

  if (entries.error) return <p className="text-red-600 dark:text-red-400">{String(entries.error)}</p>;
  if (!entries.data) return <p className="text-gray-500 dark:text-gray-400">불러오는 중…</p>;
  const { configPath, error: configError } = entries.data;
  const disabled = busy || configError !== null;

  return (
    <div className="space-y-3">
      {configError ? (
        <div className="rounded border border-red-200 dark:border-red-900 bg-red-50 dark:bg-red-950 px-3 py-2 text-xs">
          {/* TOML parse 오류는 caret(^)으로 위치를 가리키므로 고정폭으로 보인다 */}
          <p className="font-mono whitespace-pre-wrap text-red-700 dark:text-red-300">{configError}</p>
          <p className="mt-1 font-mono break-all text-gray-600 dark:text-gray-400">{configPath}</p>
        </div>
      ) : (
        <ul className="max-h-72 divide-y divide-gray-200 dark:divide-gray-800 overflow-y-auto rounded border border-gray-200 dark:border-gray-800">
          {entries.data.entries.map((entry) => (
            <RepoRow key={entry.path} entry={entry} disabled={disabled} run={run} />
          ))}
          {entries.data.entries.length === 0 && (
            <li className="px-3 py-3 text-gray-500 dark:text-gray-400">등록된 레포가 없습니다</li>
          )}
        </ul>
      )}
      <div className="flex items-center gap-3">
        <button className={BUTTON} disabled={disabled} onClick={() => run(addRepo)}>
          ＋ Repository 추가…
        </button>
        {error && <p className="min-w-0 flex-1 text-xs break-all text-red-600 dark:text-red-400">{error}</p>}
      </div>
    </div>
  );
}

/** 목록과 base 후보 (branch가 생기거나 지워졌을 수 있다) */
function refresh(queryClient: QueryClient) {
  queryClient.invalidateQueries({ queryKey: ENTRIES_KEY });
  queryClient.invalidateQueries({ queryKey: CANDIDATES_KEY });
}

function RepoRow({ entry, disabled, run }: { entry: RepoEntry; disabled: boolean; run: Run }) {
  const queryClient = useQueryClient();
  const [confirming, setConfirming] = useState(false);
  // 줄이 보일 때 미리 읽는다. native popup은 열린 동안 option이 늘어도 반영되지 않아서, 키보드·accessibility로 바로 열어도 다 보이게
  const candidates = useQuery({
    queryKey: [...CANDIDATES_KEY, entry.path],
    queryFn: () => listBaseCandidates(entry.path),
    enabled: !entry.error,
  });

  const changeBase = (value: string) => {
    const base = value === '' ? null : value;
    // 저장·반영이 끝날 때까지 select가 옛 값으로 돌아가 보이지 않게 먼저 바꿔 둔다 (실패하면 다시 읽어 되돌아간다)
    queryClient.setQueryData<RepoEntries>(ENTRIES_KEY, (old) =>
      old && { ...old, entries: old.entries.map((e) => (e.path === entry.path ? { ...e, base } : e)) },
    );
    run(() => setRepoBase(entry.path, base));
  };

  return (
    <li className="flex items-center gap-3 px-3 py-2">
      <div className="min-w-0 flex-1">
        <p className="truncate font-medium" title={entry.name}>
          {entry.name}
        </p>
        <p className="truncate font-mono text-xs text-gray-500 dark:text-gray-400" title={entry.path}>
          {entry.path}
        </p>
        {entry.error && <p className="text-xs break-all text-amber-700 dark:text-amber-300">⚠ {entry.error}</p>}
      </div>
      {confirming ? (
        <div className="flex shrink-0 items-center gap-2 text-xs">
          <span className="flex flex-col items-end">
            <span>삭제할까요?</span>
            {entry.unsentComments > 0 && (
              <span className="text-blue-600 dark:text-blue-400">미전송 코멘트 {entry.unsentComments}개가 남습니다</span>
            )}
          </span>
          <button className={BUTTON} onClick={() => setConfirming(false)}>
            취소
          </button>
          <button
            className="rounded border border-red-300 dark:border-red-800 px-2 py-0.5 text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-950 disabled:opacity-50"
            disabled={disabled}
            onClick={() => run(() => removeRepo(entry.path))}
          >
            삭제
          </button>
        </div>
      ) : (
        <div className="flex shrink-0 items-center gap-2 text-xs">
          <span className="text-gray-500 dark:text-gray-400">base</span>
          <select
            className="max-w-44 rounded border border-gray-300 dark:border-gray-700 bg-white dark:bg-gray-900 px-1 py-0.5 disabled:opacity-50"
            title={entry.base === null ? 'PR이 있는 worktree는 PR base가 우선합니다' : undefined}
            value={entry.base ?? ''}
            disabled={disabled || entry.error !== null}
            onChange={(e) => changeBase(e.target.value)}
          >
            <option value="">{autoLabel(entry.autoBase)}</option>
            {baseOptions(candidates.data ?? [], entry.base).map((b) => (
              <option key={b} value={b}>
                {b}
              </option>
            ))}
          </select>
          <button className={BUTTON} disabled={disabled} onClick={() => setConfirming(true)}>
            삭제
          </button>
        </div>
      )}
    </li>
  );
}
