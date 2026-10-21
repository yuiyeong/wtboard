import { create } from 'zustand';

/**
 * 다른 view·세션으로 옮긴 뒤 파일로 스크롤하는 요청. 파일 목록이 query로 늦게 오므로 바로 스크롤할 수 없어
 * 여기 두고, `WorktreeView`가 목록에 그 파일이 들어오면 `takeScroll`로 가져가 스크롤한다. slot 하나 (저장하지 않는다)
 */
interface NavState {
  pendingScroll: { worktreeId: string; path: string } | null;
}

export const useNavStore = create<NavState>()(() => ({ pendingScroll: null }));

export function requestScroll(worktreeId: string, path: string) {
  useNavStore.setState({ pendingScroll: { worktreeId, path } });
}

/** 이 worktree의 요청이고 목록(`listed`)에 그 파일이 있으면 path를 돌려주고 요청을 지운다. 아니면 null (요청은 남는다) */
export function takeScroll(worktreeId: string, listed: Set<string>): string | null {
  const pending = useNavStore.getState().pendingScroll;
  if (!pending || pending.worktreeId !== worktreeId || !listed.has(pending.path)) return null;
  useNavStore.setState({ pendingScroll: null });
  return pending.path;
}
