import { describe, expect, it } from 'vitest';
import { parseTheme } from './theme';

describe('parseTheme', () => {
  it("'light'만 light", () => {
    expect(parseTheme('light')).toBe('light');
  });

  it('그 밖의 값은 dark', () => {
    for (const v of ['dark', undefined, null, 'x', 1]) expect(parseTheme(v)).toBe('dark');
  });
});
