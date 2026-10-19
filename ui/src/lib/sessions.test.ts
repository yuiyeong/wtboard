import { describe, expect, it } from 'vitest';
import type { SessionSummary } from '../api';
import { effectiveView, filesKey, patchKey, pickSession, sessionLabel, tabDisabled, turnLabel } from './sessions';

const session = (id: string, overrides: Partial<SessionSummary> = {}): SessionSummary => ({
  id,
  startedAt: 0,
  lastEventAt: 0,
  active: false,
  turn: 1,
  turnOpen: false,
  touchedFiles: 0,
  ...overrides,
});

describe('pickSession', () => {
  const list = [session('new'), session('old')];

  it('기억한 session이 목록에 있으면 그것을 고른다', () => {
    expect(pickSession(list, 'old')?.id).toBe('old');
  });

  it('기억한 session이 없어졌으면 첫 번째(최근 활동)를 고른다', () => {
    expect(pickSession(list, 'gone')?.id).toBe('new');
  });

  it('기억한 것이 없으면 첫 번째를 고른다', () => {
    expect(pickSession(list, undefined)?.id).toBe('new');
  });

  it('목록이 비면 null', () => {
    expect(pickSession([], 'old')).toBeNull();
  });
});

describe('effectiveView', () => {
  it('session이 없으면 branch', () => {
    expect(effectiveView('session', null)).toBe('branch');
    expect(effectiveView('turn', null)).toBe('branch');
  });

  it('session이 있으면 고른 view 그대로', () => {
    expect(effectiveView('turn', session('s'))).toBe('turn');
    expect(effectiveView('branch', session('s'))).toBe('branch');
  });
});

describe('tabDisabled', () => {
  it('session이 없으면 세션 탭 두 개만 비활성', () => {
    expect(tabDisabled('branch', null)).toBe(false);
    expect(tabDisabled('session', null)).toBe(true);
    expect(tabDisabled('turn', null)).toBe(true);
  });

  it('session이 있으면 모두 활성', () => {
    for (const v of ['branch', 'session', 'turn'] as const) expect(tabDisabled(v, session('s'))).toBe(false);
  });
});

describe('query key', () => {
  it('filesKey는 sessionId가 없으면 null로 채운다', () => {
    expect(filesKey('w', 'branch', null)).toEqual(['files', 'w', 'branch', null]);
    expect(filesKey('w', 'session', 's')).toEqual(['files', 'w', 'session', 's']);
  });

  it('patchKey는 path 뒤에 view와 sessionId가 온다', () => {
    expect(patchKey('w', 'a.ts', 'turn', 's')).toEqual(['patch', 'w', 'a.ts', 'turn', 's']);
    expect(patchKey('w', 'a.ts', 'branch', null)).toEqual(['patch', 'w', 'a.ts', 'branch', null]);
  });
});

describe('turnLabel', () => {
  it('turn 번호', () => {
    expect(turnLabel(session('s', { turn: 3 }))).toBe('#3');
  });

  it('진행 중이면 표시를 붙인다', () => {
    expect(turnLabel(session('s', { turn: 3, turnOpen: true }))).toBe('#3 진행 중');
  });
});

describe('sessionLabel', () => {
  const at = new Date(2026, 9, 19, 9, 5).getTime(); // 로컬 시각

  it('시작 시각 HH:MM', () => {
    expect(sessionLabel(session('s', { startedAt: at }))).toBe('09:05 시작');
  });

  it('활성이면 ●를 붙인다', () => {
    expect(sessionLabel(session('s', { startedAt: at, active: true }))).toBe('09:05 시작 ●');
  });
});
