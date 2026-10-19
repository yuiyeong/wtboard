import { describe, expect, it } from 'vitest';
import { fileFixture, worktreeFixture } from './fixtures';
import { countLabel, initiallyOpen, nextOpen, viewedErrorText, viewedProgress, withViewed } from './viewed';

describe('viewedProgress', () => {
  it('viewed인 파일만 센다', () => {
    const files = [
      fileFixture({ path: 'a', viewed: 'none' }),
      fileFixture({ path: 'b', viewed: 'viewed' }),
      fileFixture({ path: 'c', viewed: 'changed' }),
    ];
    expect(viewedProgress(files)).toEqual({ viewed: 1, total: 3 });
  });

  it('빈 목록은 0/0', () => {
    expect(viewedProgress([])).toEqual({ viewed: 0, total: 0 });
  });
});

describe('initiallyOpen', () => {
  it('안 본 파일과 다시 바뀐 파일은 펼친다', () => {
    expect(initiallyOpen(fileFixture({ viewed: 'none' }))).toBe(true);
    expect(initiallyOpen(fileFixture({ viewed: 'changed' }))).toBe(true);
  });

  it('본 파일은 접는다', () => {
    expect(initiallyOpen(fileFixture({ viewed: 'viewed' }))).toBe(false);
  });

  it('collapsed 파일은 접는다', () => {
    expect(initiallyOpen(fileFixture({ viewed: 'none', collapsed: true }))).toBe(false);
  });
});

describe('nextOpen', () => {
  it('체크하면 접힌다', () => {
    expect(nextOpen('none', 'viewed', true)).toBe(false);
    expect(nextOpen('changed', 'viewed', true)).toBe(false);
  });

  it('해제하면 펼친다', () => {
    expect(nextOpen('viewed', 'none', false)).toBe(true);
  });

  it('다시 바뀌면 펼친다', () => {
    expect(nextOpen('viewed', 'changed', false)).toBe(true);
  });

  it('상태가 그대로면 현재 값을 유지한다', () => {
    expect(nextOpen('none', 'none', false)).toBe(false);
    expect(nextOpen('none', 'none', true)).toBe(true);
  });

  it('changed에서 해제해도 펼침을 유지한다', () => {
    expect(nextOpen('changed', 'none', true)).toBe(true);
  });
});

describe('withViewed', () => {
  const files = [fileFixture({ path: 'a' }), fileFixture({ path: 'b', viewed: 'changed' })];

  it('true면 대상 파일만 viewed가 되고 나머지는 같은 참조다', () => {
    const next = withViewed(files, 'a', true);
    expect(next[0].viewed).toBe('viewed');
    expect(next[1]).toBe(files[1]);
  });

  it('false면 대상 파일이 none이 된다', () => {
    const next = withViewed(files, 'b', false);
    expect(next[1].viewed).toBe('none');
    expect(next[0]).toBe(files[0]);
  });

  it('원본을 바꾸지 않는다', () => {
    withViewed(files, 'a', true);
    expect(files[0].viewed).toBe('none');
  });
});

describe('countLabel', () => {
  it('안 본 파일이 있으면 unviewed/changed', () => {
    expect(countLabel(worktreeFixture({ unviewedFiles: 3, changedFiles: 14 }))).toBe('3/14');
  });

  it('안 본 파일이 없으면 changed만', () => {
    expect(countLabel(worktreeFixture({ unviewedFiles: 0, changedFiles: 14 }))).toBe('14');
  });
});

describe('viewedErrorText', () => {
  it('Tauri command가 돌려준 문자열 오류를 그대로 붙인다', () => {
    expect(viewedErrorText('변경 목록에 없는 파일: a.py')).toBe('봤음 저장 실패: 변경 목록에 없는 파일: a.py');
  });
});
