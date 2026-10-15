import { create } from 'zustand';

export type ViewType = 'split' | 'unified';

interface UiState {
  viewType: ViewType;
  setViewType: (viewType: ViewType) => void;
}

export const useUiStore = create<UiState>((set) => ({
  viewType: 'split',
  setViewType: (viewType) => set({ viewType }),
}));
