import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { View } from './api';
import type { Selection } from './lib/selection';

export type ViewType = 'split' | 'unified';

interface UiState {
  viewType: ViewType;
  setViewType: (viewType: ViewType) => void;
  selection: Selection | null;
  select: (selection: Selection) => void;
  /** worktree별로 고른 탭 */
  views: Record<string, View>;
  setView: (worktreeId: string, view: View) => void;
  /** worktree별로 고른 session id */
  sessions: Record<string, string>;
  selectSession: (worktreeId: string, id: string) => void;
}

export const useUiStore = create<UiState>()(
  persist(
    (set) => ({
      viewType: 'split',
      setViewType: (viewType) => set({ viewType }),
      selection: null,
      select: (selection) => set({ selection }),
      views: {},
      setView: (worktreeId, view) => set((s) => ({ views: { ...s.views, [worktreeId]: view } })),
      sessions: {},
      selectSession: (worktreeId, id) => set((s) => ({ sessions: { ...s.sessions, [worktreeId]: id } })),
    }),
    { name: 'wtboard-ui' }, // 기본 저장소는 localStorage
  ),
);
