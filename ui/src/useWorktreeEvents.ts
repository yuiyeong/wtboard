import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useQueryClient } from '@tanstack/react-query';
import type { WorktreeChanged } from './api';
import { keysToInvalidate } from './lib/events';

/** worktree-changed event가 오면 관련 query만 무효화한다. 화면에 보이는 query만 다시 불러온다. */
export function useWorktreeEvents() {
  const queryClient = useQueryClient();
  useEffect(() => {
    const unlisten = listen<WorktreeChanged>('worktree-changed', ({ payload }) => {
      for (const queryKey of keysToInvalidate(payload)) queryClient.invalidateQueries({ queryKey });
    });
    return () => {
      unlisten.then((off) => off());
    };
  }, [queryClient]);
}
