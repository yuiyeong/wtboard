import { describe, expect, it } from 'vitest';
import type { WorktreeSummary } from '../api';
import { showFsmonitorBanner } from './banner';

const w = (slowListMs: number | null, fsmonitor: boolean): WorktreeSummary => ({
  id: 'w',
  path: '/w',
  branch: 'main',
  base: 'origin/main',
  baseSource: 'origin',
  changedFiles: 0,
  profile: 'default',
  lastChangedAt: null,
  slowListMs,
  fsmonitor,
  error: null,
});

describe('showFsmonitorBanner', () => {
  it('느리고 fsmonitor가 꺼져 있으면 보인다', () => {
    expect(showFsmonitorBanner(w(820, false), 'r', [])).toBe(true);
  });

  it('빠르거나 fsmonitor가 켜져 있으면 안 보인다', () => {
    expect(showFsmonitorBanner(w(null, false), 'r', [])).toBe(false);
    expect(showFsmonitorBanner(w(820, true), 'r', [])).toBe(false);
  });

  it('그 레포에서 닫았으면 안 보인다', () => {
    expect(showFsmonitorBanner(w(820, false), 'r', ['r'])).toBe(false);
    expect(showFsmonitorBanner(w(820, false), 'other', ['r'])).toBe(true);
  });
});
