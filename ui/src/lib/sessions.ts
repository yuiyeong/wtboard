import type { SessionSummary, View } from '../api';

/** 기억한 session이 목록에 있으면 그것, 없으면 첫 번째(최근 활동), 목록이 비면 null */
export function pickSession(sessions: SessionSummary[], remembered: string | undefined): SessionSummary | null {
  return sessions.find((s) => s.id === remembered) ?? sessions[0] ?? null;
}

/** session이 없으면 어느 탭을 골랐어도 branch를 보여 준다 */
export function effectiveView(view: View, session: SessionSummary | null): View {
  return session ? view : 'branch';
}

export const filesKey = (worktreeId: string, view: View, sessionId: string | null) =>
  ['files', worktreeId, view, sessionId ?? null] as const;

export const patchKey = (worktreeId: string, path: string, view: View, sessionId: string | null) =>
  ['patch', worktreeId, path, view, sessionId ?? null] as const;

/** `#3` 또는 `#3 진행 중` */
export function turnLabel(s: SessionSummary): string {
  return `#${s.turn}${s.turnOpen ? ' 진행 중' : ''}`;
}

/** 세션 select의 항목. `14:02 시작`, 활성이면 `14:02 시작 ●` */
export function sessionLabel(s: SessionSummary): string {
  const d = new Date(s.startedAt);
  const hhmm = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  return `${hhmm} 시작${s.active ? ' ●' : ''}`;
}
