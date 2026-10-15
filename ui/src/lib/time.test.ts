import { describe, expect, it } from 'vitest';
import { formatRelative } from './time';

const now = 1_800_000_000_000;

describe('formatRelative', () => {
  it('1분 안은 방금', () => {
    expect(formatRelative(now - 10_000, now)).toBe('방금');
    expect(formatRelative(now + 5_000, now)).toBe('방금'); // 시계 오차
  });

  it('분, 시간, 일 단위', () => {
    expect(formatRelative(now - 3 * 60_000, now)).toBe('3분 전');
    expect(formatRelative(now - 2 * 3_600_000, now)).toBe('2시간 전');
    expect(formatRelative(now - 3 * 86_400_000, now)).toBe('3일 전');
  });
});
