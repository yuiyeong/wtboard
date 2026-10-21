import { beforeEach, describe, expect, it } from 'vitest';
import { requestScroll, takeScroll, useNavStore } from './nav';

beforeEach(() => useNavStore.setState({ pendingScroll: null }));

describe('스크롤 예약 (다른 view로 옮긴 뒤 파일 목록이 오면 스크롤)', () => {
  it('목록에 그 파일이 있으면 돌려주고 요청을 지운다', () => {
    requestScroll('w', 'a.ts');
    expect(takeScroll('w', new Set(['a.ts', 'b.ts']))).toBe('a.ts');
    expect(useNavStore.getState().pendingScroll).toBeNull();
  });

  it('목록에 아직 없으면 null이고 요청이 남는다', () => {
    requestScroll('w', 'a.ts');
    expect(takeScroll('w', new Set(['b.ts']))).toBeNull();
    expect(takeScroll('w', new Set(['a.ts']))).toBe('a.ts');
  });

  it('다른 worktree의 목록은 요청을 건드리지 않는다', () => {
    requestScroll('w', 'a.ts');
    expect(takeScroll('other', new Set(['a.ts']))).toBeNull();
    expect(useNavStore.getState().pendingScroll).toEqual({ worktreeId: 'w', path: 'a.ts' });
  });

  it('새 요청이 이전 요청을 바꾼다 (slot 하나)', () => {
    requestScroll('w', 'a.ts');
    requestScroll('w2', 'c.ts');
    expect(takeScroll('w', new Set(['a.ts']))).toBeNull();
    expect(takeScroll('w2', new Set(['c.ts']))).toBe('c.ts');
  });
});
