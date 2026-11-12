import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getSettings } from './api';
import { parseTheme, type Theme } from './lib/theme';

/**
 * `<html data-theme>`을 저장된 theme에 맞춘다. 처음 값은 창을 만들 때의 init script(`index.html`)가 정해 둔다.
 * webview를 다시 읽으면 init script가 창을 만들 때의 값으로 돌기 때문에 저장 값을 한 번 더 읽고, 그 뒤로는 `theme-changed`를 따른다.
 * 지금 theme을 돌려준다
 */
export function useTheme(): Theme {
  const [theme, setTheme] = useState(() => parseTheme(document.documentElement.dataset.theme));
  useEffect(() => {
    let changed = false;
    const update = (next: Theme) => {
      document.documentElement.dataset.theme = next;
      setTheme(next);
    };
    getSettings()
      .then((s) => {
        // 읽는 사이에 event가 왔으면 그 값이 더 새롭다
        if (!changed) update(parseTheme(s.theme));
      })
      .catch((e) => console.warn('[wtboard] 설정을 읽을 수 없음', e));
    const unlisten = listen<{ theme: unknown }>('theme-changed', ({ payload }) => {
      changed = true;
      update(parseTheme(payload.theme));
    });
    return () => {
      unlisten.then((off) => off());
    };
  }, []);
  return theme;
}
