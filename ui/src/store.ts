import { create } from 'zustand';
import { createJSONStorage, persist } from 'zustand/middleware';
import type { Selection } from './lib/selection';

export type ViewType = 'split' | 'unified';

interface UiState {
  viewType: ViewType;
  setViewType: (viewType: ViewType) => void;
  selection: Selection | null;
  select: (selection: Selection) => void;
}

export const useUiStore = create<UiState>()(
  persist(
    (set) => ({
      viewType: 'split',
      setViewType: (viewType) => set({ viewType }),
      selection: null,
      select: (selection) => set({ selection }),
    }),
    { name: 'wtboard-ui', storage: createJSONStorage(() => localStorage) },
  ),
);
