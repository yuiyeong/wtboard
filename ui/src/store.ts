import { create } from 'zustand';
import { persist } from 'zustand/middleware';
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
    { name: 'wtboard-ui' }, // 기본 저장소는 localStorage
  ),
);
