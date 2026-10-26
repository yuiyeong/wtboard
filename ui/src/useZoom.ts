import { useEffect } from 'react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { zoomForKey } from './lib/zoom';
import { useUiStore } from './store';

function applyZoom(zoom: number) {
  getCurrentWebview().setZoom(zoom).catch((e) => console.warn('[wtboard] zoom 실패', e));
}

/**
 * Cmd = / - / 0으로 화면 전체를 확대·축소한다. 배율은 store에 남아 다시 켜도 유지된다.
 * zoom을 구독하지 않는다: 구독하면 키를 누를 때마다 App 아래 diff까지 다시 렌더된다
 */
export function useZoom() {
  useEffect(() => {
    applyZoom(useUiStore.getState().zoom);
    const onKeyDown = (e: KeyboardEvent) => {
      const { zoom, setZoom } = useUiStore.getState();
      const next = zoomForKey(e, zoom);
      if (next === null) return;
      e.preventDefault();
      setZoom(next);
      applyZoom(next);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);
}
