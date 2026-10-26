import { useEffect } from 'react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { zoomForKey } from './lib/zoom';
import { useUiStore } from './store';

/** Cmd = / - / 0으로 화면 전체를 확대·축소한다. 배율은 store에 남아 다시 켜도 유지된다 */
export function useZoom() {
  const zoom = useUiStore((s) => s.zoom);
  useEffect(() => {
    getCurrentWebview().setZoom(zoom).catch((e) => console.warn('[wtboard] zoom 실패', e));
  }, [zoom]);
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const { zoom, setZoom } = useUiStore.getState();
      const next = zoomForKey(e, zoom);
      if (next === null) return;
      e.preventDefault();
      setZoom(next);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);
}
