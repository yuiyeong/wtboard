import type { SessionSummary, View } from '../api';
import { sessionLabel, turnLabel } from '../lib/sessions';

const NO_SESSION = '이 worktree의 Claude Code 세션 기록이 없습니다';

interface Props {
  /** 실제로 보여 주는 view (`effectiveView`) */
  view: View;
  sessions: SessionSummary[];
  /** 고른 세션. 없으면 세션 탭 두 개가 비활성 */
  session: SessionSummary | null;
  onView: (view: View) => void;
  onSession: (id: string) => void;
}

/** `브랜치 전체` / `이번 세션` (세션이 여러 개면 select) / `마지막 응답 #n` */
export function ViewTabs({ view, sessions, session, onView, onSession }: Props) {
  const tab = (v: View, label: string) => {
    const disabled = v !== 'branch' && !session;
    return (
      <button
        onClick={() => onView(v)}
        disabled={disabled}
        title={disabled ? NO_SESSION : undefined}
        className={`rounded px-2 py-1 disabled:cursor-default disabled:text-gray-300 ${
          view === v ? 'bg-gray-800 text-white' : 'hover:bg-gray-100'
        }`}
      >
        {label}
      </button>
    );
  };
  return (
    <div className="flex items-center gap-1">
      {tab('branch', '브랜치 전체')}
      {tab('session', '이번 세션')}
      {session && sessions.length > 1 && (
        <select
          value={session.id}
          onChange={(e) => onSession(e.target.value)}
          className="rounded border border-gray-300 px-1 py-0.5 text-xs"
        >
          {sessions.map((s) => (
            <option key={s.id} value={s.id}>
              {sessionLabel(s)}
            </option>
          ))}
        </select>
      )}
      {tab('turn', session ? `마지막 응답 ${turnLabel(session)}` : '마지막 응답')}
    </div>
  );
}
