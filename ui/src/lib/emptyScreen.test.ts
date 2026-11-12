import { describe, expect, it } from 'vitest';
import { emptyScreen } from './emptyScreen';

describe('emptyScreen', () => {
  it('config에 레포가 0개면 등록 안내', () => {
    expect(emptyScreen(0, 0)).toBe('noRepos');
  });

  it('등록은 있는데 하나도 열지 못하면 오류 안내', () => {
    expect(emptyScreen(2, 0)).toBe('unopened');
  });

  it('하나라도 열렸으면 빈 화면이 아니다', () => {
    expect(emptyScreen(1, 1)).toBeNull();
  });
});
