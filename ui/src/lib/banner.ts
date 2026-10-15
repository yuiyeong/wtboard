import type { WorktreeSummary } from '../api';

/** 앱은 레포에 쓰지 않으므로 사용자가 직접 실행할 명령을 보여 준다 */
export const FSMONITOR_COMMANDS = ['git config core.fsmonitor true', 'git config core.untrackedCache true'];

/** 목록 계산이 느리고, fsmonitor가 꺼져 있고, 이 레포의 banner를 닫지 않았을 때 */
export function showFsmonitorBanner(w: WorktreeSummary, dismissed: boolean): boolean {
  return w.slowListMs !== null && !w.fsmonitor && !dismissed;
}
