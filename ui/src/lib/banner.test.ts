import { describe, expect, it } from 'vitest';
import { showFsmonitorBanner } from './banner';
import { worktreeFixture } from './fixtures';

const w = (slowListMs: number | null, fsmonitor: boolean) => worktreeFixture({ slowListMs, fsmonitor });

describe('showFsmonitorBanner', () => {
  it('느리고 fsmonitor가 꺼져 있으면 보인다', () => {
    expect(showFsmonitorBanner(w(820, false), false)).toBe(true);
  });

  it('빠르거나 fsmonitor가 켜져 있으면 안 보인다', () => {
    expect(showFsmonitorBanner(w(null, false), false)).toBe(false);
    expect(showFsmonitorBanner(w(820, true), false)).toBe(false);
  });

  it('그 레포에서 닫았으면 안 보인다', () => {
    expect(showFsmonitorBanner(w(820, false), true)).toBe(false);
  });
});
