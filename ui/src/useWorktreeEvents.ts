import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useQueryClient } from '@tanstack/react-query';
import type { ReposResponse, SessionUpdated, WorktreeChanged } from './api';
import { filtersForSessionUpdated, keysToInvalidate } from './lib/events';
import { selectionForWorktree } from './lib/selection';
import { useUiStore } from './store';

/**
 * worktree-changed·session-updated event가 오면 관련 query만 무효화한다. worktree 목록이 바뀌면 레포 목록을 다시 읽는다.
 * select-worktree(메뉴바)가 오면 그 worktree를 선택한다.
 */
export function useWorktreeEvents() {
  const queryClient = useQueryClient();
  useEffect(() => {
    const unlistenChanged = listen<WorktreeChanged>('worktree-changed', ({ payload }) => {
      for (const queryKey of keysToInvalidate(payload)) queryClient.invalidateQueries({ queryKey });
    });
    const unlistenUpdated = listen('worktrees-updated', () => {
      queryClient.invalidateQueries({ queryKey: ['repos'] });
    });
    const unlistenSession = listen<SessionUpdated>('session-updated', ({ payload }) => {
      for (const filters of filtersForSessionUpdated(payload)) queryClient.invalidateQueries(filters);
    });
    // 메뉴바에서 worktree를 고르면 그 worktree로 이동한다
    const unlistenSelect = listen<{ worktreeId: string }>('select-worktree', ({ payload }) => {
      const repos = queryClient.getQueryData<ReposResponse>(['repos'])?.repos ?? [];
      const selection = selectionForWorktree(repos, payload.worktreeId);
      if (selection) useUiStore.getState().select(selection);
    });
    return () => {
      unlistenChanged.then((off) => off());
      unlistenSession.then((off) => off());
      unlistenUpdated.then((off) => off());
      unlistenSelect.then((off) => off());
    };
  }, [queryClient]);
}
