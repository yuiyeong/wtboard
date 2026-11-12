export type Theme = 'dark' | 'light';

/** 저장 값·event·`data-theme`을 해석한다. 'light'가 아니면 dark (Rust `Theme::from_setting`과 같다) */
export function parseTheme(value: unknown): Theme {
  return value === 'light' ? 'light' : 'dark';
}
