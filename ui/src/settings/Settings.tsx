import { setTheme } from '../api';
import type { Theme } from '../lib/theme';
import { useTheme } from '../useTheme';

const THEMES: { value: Theme; label: string }[] = [
  { value: 'dark', label: 'Dark' },
  { value: 'light', label: 'Light' },
];

/** 설정창 화면 (label `settings` 창) */
export function Settings() {
  const theme = useTheme();
  return (
    <div className="space-y-6 p-6 text-sm">
      <div className="flex items-center gap-6">
        <span className="w-28 font-medium">Theme</span>
        <div className="flex rounded-md border border-gray-300 dark:border-gray-700 p-0.5">
          {THEMES.map((t) => (
            <button
              key={t.value}
              aria-pressed={theme === t.value}
              onClick={() => setTheme(t.value).catch((e) => console.warn('[wtboard] theme을 바꿀 수 없음', e))}
              className={
                theme === t.value
                  ? 'rounded bg-gray-800 dark:bg-gray-700 px-4 py-1 text-white'
                  : 'rounded px-4 py-1 text-gray-500 dark:text-gray-400 hover:text-gray-800 dark:hover:text-gray-200'
              }
            >
              {t.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
