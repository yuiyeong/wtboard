import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useQueryClient } from '@tanstack/react-query';
import type { SessionUpdated, WorktreeChanged } from './api';
import { filtersForSessionUpdated, keysToInvalidate } from './lib/events';

/** worktree-changed·session-updated event가 오면 관련 query만 무효화한다. worktree 목록이 바뀌면 레포 목록을 다시 읽는다. */
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
    return () => {
      unlistenChanged.then((off) => off());
      unlistenSession.then((off) => off());
      unlistenUpdated.then((off) => off());
    };
  }, [queryClient]);
}
