import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { Selection } from './lib/selection';

export type ViewType = 'split' | 'unified';

interface UiState {
  viewType: ViewType;
  setViewType: (viewType: ViewType) => void;
  selection: Selection | null;
  select: (selection: Selection) => void;
  /** fsmonitor banner를 닫은 레포 id */
  dismissedBanners: string[];
  dismissBanner: (repoId: string) => void;
}

export const useUiStore = create<UiState>()(
  persist(
    (set) => ({
      viewType: 'split',
      setViewType: (viewType) => set({ viewType }),
      selection: null,
      select: (selection) => set({ selection }),
      dismissedBanners: [],
      dismissBanner: (repoId) => set((s) => ({ dismissedBanners: [...s.dismissedBanners, repoId] })),
    }),
    // 기본 저장소는 localStorage. banner 닫기는 앱 실행 중에만 기억한다
    { name: 'wtboard-ui', partialize: (s) => ({ viewType: s.viewType, selection: s.selection }) },
  ),
);
