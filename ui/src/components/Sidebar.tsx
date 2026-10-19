import type { EntryError, Profile, RepoSummary } from '../api';
import { worktreeLabel, type Selection } from '../lib/selection';
import { formatRelative } from '../lib/time';
import { countLabel } from '../lib/viewed';
import { useNow } from '../lib/useNow';

const PROFILE_BADGE: Record<Profile, string | null> = { python: 'Py', node: 'Node', flutter: 'Flutter', default: null };

interface Props {
  repos: RepoSummary[];
  errors: EntryError[];
  selectedId: string | null;
  onSelect: (selection: Selection) => void;
}

export function Sidebar({ repos, errors, selectedId, onSelect }: Props) {
  const now = useNow(30_000);
  return (
    <nav className="w-56 shrink-0 overflow-y-auto border-r border-gray-200 bg-gray-50 py-2 text-sm">
      {repos.map((repo) => {
        const badge = PROFILE_BADGE[repo.profile];
        return (
          <section key={repo.id} className="mb-2">
            <h2 className="flex items-center gap-2 px-3 py-1 font-semibold text-gray-700" title={repo.id}>
              <span className="flex-1 truncate">{repo.name}</span>
              {badge && <span className="rounded bg-gray-200 px-1 text-[10px] font-medium text-gray-600">{badge}</span>}
            </h2>
            <ul>
              {repo.worktrees.map((w) => (
                <li key={w.id}>
                  <button
                    onClick={() => onSelect({ repoId: repo.id, worktreeId: w.id })}
                    title={w.path}
                    className={`flex w-full flex-col py-1 pr-3 pl-5 text-left ${
                      w.id === selectedId ? 'bg-blue-100' : 'hover:bg-gray-100'
                    }`}
                  >
                    <span className="flex w-full items-center gap-2">
                      {w.error && <span className="text-amber-600">⚠</span>}
                      {w.activeSession && (
                        <span className="text-[10px] text-green-600" title="Claude Code 세션 진행 중">
                          ●
                        </span>
                      )}
                      <span className="flex-1 truncate font-mono text-xs">{worktreeLabel(w)}</span>
                      {w.unsentComments > 0 && (
                        <span className="text-xs text-blue-600 tabular-nums" title="미전송 코멘트">
                          💬{w.unsentComments}
                        </span>
                      )}
                      {!w.error && (
                        <span
                          className={`text-xs tabular-nums ${w.unviewedFiles > 0 ? 'text-gray-800' : 'text-gray-500'}`}
                        >
                          {countLabel(w)}
                        </span>
                      )}
                    </span>
                    {w.lastChangedAt !== null && (
                      <span className="text-[11px] text-gray-400">{formatRelative(w.lastChangedAt, now)}</span>
                    )}
                  </button>
                </li>
              ))}
            </ul>
          </section>
        );
      })}
      {errors.length > 0 && (
        <ul className="mt-2 border-t border-gray-200 px-3 pt-2 text-xs text-amber-700">
          {errors.map((e) => (
            <li key={`${e.path}:${e.message}`} className="mb-1 break-all">
              ⚠ {e.path}: {e.message}
            </li>
          ))}
        </ul>
      )}
    </nav>
  );
}
