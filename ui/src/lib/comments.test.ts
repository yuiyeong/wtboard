import { describe, expect, it } from 'vitest';
import { getChangeKey, parseDiff, type ChangeData } from 'react-diff-view';
import { anchorOf, byPath, firstLine, placeComments, sortForList, statusLabel, unsentByPath } from './comments';
import { commentFixture } from './fixtures';

const DIFF = [
  'diff --git a/a.ts b/a.ts',
  '--- a/a.ts',
  '+++ b/a.ts',
  '@@ -1,3 +1,3 @@',
  ' ctx',
  '-old line',
  '+new line',
  ' tail',
  '',
].join('\n');

/** 실제 parseDiff 결과: [ctx, old line(delete), new line(insert), tail] */
const changes: ChangeData[] = parseDiff(DIFF)[0].hunks[0].changes;
const [ctx, del, ins, tail] = changes;

describe('anchorOf', () => {
  it('delete는 old·옛 줄 번호, insert는 new, normal은 어느 쪽을 눌러도 new·새 줄 번호', () => {
    expect(anchorOf(del, 'old')).toEqual({ side: 'old', line: 2, snippet: 'old line' });
    expect(anchorOf(ins, 'new')).toEqual({ side: 'new', line: 2, snippet: 'new line' });
    expect(anchorOf(ctx, 'new')).toEqual({ side: 'new', line: 1, snippet: 'ctx' });
    expect(anchorOf(tail, 'old')).toEqual({ side: 'new', line: 3, snippet: 'tail' });
  });

  it('snippet에는 +/-/공백 접두가 없다 (parseDiff content 그대로)', () => {
    expect(anchorOf(del, 'old').snippet).not.toMatch(/^[-+ ]/);
    expect(anchorOf(ins, 'new').snippet).not.toMatch(/^[-+ ]/);
  });
});

describe('긴 줄 (서버는 snippet을 500 code point에서 자른다)', () => {
  const longDiff = (content: string) =>
    ['diff --git a/a.ts b/a.ts', '--- a/a.ts', '+++ b/a.ts', '@@ -1 +1 @@', '-x', `+${content}`, ''].join('\n');
  const longChange = (content: string) => parseDiff(longDiff(content))[0].hunks[0].changes[1];
  const serverTruncate = (s: string) => Array.from(s).slice(0, 500).join('');

  it('501자 넘는 줄도 anchorOf snippet이 500자이고 서버가 자른 코멘트가 맞는다', () => {
    const change = longChange('a'.repeat(700));
    const { snippet, line, side } = anchorOf(change, 'new');
    expect(snippet).toBe('a'.repeat(500));
    const c = commentFixture({ side, line, snippet: serverTruncate('a'.repeat(700)) });
    expect(placeComments([c], [change], 'branch').byChangeKey.get(getChangeKey(change))).toEqual([c]);
  });

  it('경계에 astral 문자(이모지)가 있어도 code point로 센다', () => {
    const content = 'a'.repeat(499) + '😀' + 'tail';
    const change = longChange(content);
    expect(anchorOf(change, 'new').snippet).toBe('a'.repeat(499) + '😀');
    const c = commentFixture({ side: 'new', line: 1, snippet: serverTruncate(content) });
    expect(placeComments([c], [change], 'branch').outdated).toEqual([]);
  });

  it('긴 줄도 앞 500자가 다르면 outdated', () => {
    const change = longChange('a'.repeat(700));
    const c = commentFixture({ side: 'new', line: 1, snippet: 'b'.repeat(500) });
    expect(placeComments([c], [change], 'branch').outdated).toEqual([c]);
  });
});

describe('placeComments', () => {
  it('같은 줄·같은 내용이면 그 change에 둔다', () => {
    const a = commentFixture({ id: 1, side: 'new', line: 2, snippet: 'new line' });
    const b = commentFixture({ id: 2, side: 'old', line: 2, snippet: 'old line' });
    const c = commentFixture({ id: 3, side: 'new', line: 2, snippet: 'new line' });
    const { byChangeKey, outdated } = placeComments([a, b, c], changes, 'branch');
    expect(outdated).toEqual([]);
    expect(byChangeKey.get(getChangeKey(ins))).toEqual([a, c]);
    expect(byChangeKey.get(getChangeKey(del))).toEqual([b]);
    expect(byChangeKey.size).toBe(2);
  });

  it('문맥 줄은 새 줄 번호로 맞춘다', () => {
    const a = commentFixture({ side: 'new', line: 3, snippet: 'tail' });
    expect(placeComments([a], changes, 'branch').byChangeKey.get(getChangeKey(tail))).toEqual([a]);
  });

  it('내용이 다르거나 그 줄이 없으면 outdated', () => {
    const changed = commentFixture({ id: 1, side: 'new', line: 2, snippet: 'before edit' });
    const gone = commentFixture({ id: 2, side: 'new', line: 99, snippet: 'new line' });
    const { byChangeKey, outdated } = placeComments([changed, gone], changes, 'branch');
    expect(byChangeKey.size).toBe(0);
    expect(outdated).toEqual([changed, gone]);
  });

  it('old side는 단 view와 지금 view가 같을 때만 맞춘다', () => {
    const old = commentFixture({ side: 'old', line: 2, snippet: 'old line', view: 'branch' });
    expect(placeComments([old], changes, 'session').outdated).toEqual([old]);
    expect(placeComments([old], changes, 'branch').outdated).toEqual([]);
    // new side는 view와 무관
    const neu = commentFixture({ side: 'new', line: 2, snippet: 'new line', view: 'branch' });
    expect(placeComments([neu], changes, 'turn').outdated).toEqual([]);
  });
});

describe('sortForList', () => {
  it('미전송 → path → line → id (서버 순서)', () => {
    const sent = commentFixture({ id: 1, path: 'a.ts', line: 1, sentAt: 5 });
    const u1 = commentFixture({ id: 4, path: 'b.ts', line: 1 });
    const u2 = commentFixture({ id: 3, path: 'a.ts', line: 9 });
    const u3 = commentFixture({ id: 2, path: 'a.ts', line: 9 });
    const u4 = commentFixture({ id: 5, path: 'a.ts', line: 2 });
    expect(sortForList([sent, u1, u2, u3, u4]).map((c) => c.id)).toEqual([5, 2, 3, 4, 1]);
  });

  it('입력 배열을 바꾸지 않는다', () => {
    const list = [commentFixture({ id: 2, path: 'b' }), commentFixture({ id: 1, path: 'a' })];
    sortForList(list);
    expect(list.map((c) => c.id)).toEqual([2, 1]);
  });
});

describe('statusLabel', () => {
  const fmt = (ms: number) => `t${ms}`;
  it('미전송 / 전송됨 / 복사됨', () => {
    expect(statusLabel(commentFixture(), fmt)).toBe('미전송');
    expect(statusLabel(commentFixture({ sentAt: 7, sentTo: 'session-1' }), fmt)).toBe('전송됨 · t7');
    expect(statusLabel(commentFixture({ sentAt: 7, sentTo: 'copy' }), fmt)).toBe('복사됨 · t7');
  });

  it('기본 formatter는 로컬 시각 HH:MM (24시간, 0 채움)', () => {
    const ms = new Date(2026, 9, 19, 9, 5).getTime();
    expect(statusLabel(commentFixture({ sentAt: ms, sentTo: 'copy' }))).toBe('복사됨 · 09:05');
  });
});

describe('unsentByPath', () => {
  it('미전송만 path별로 센다', () => {
    const list = [
      commentFixture({ id: 1, path: 'a.ts' }),
      commentFixture({ id: 2, path: 'a.ts' }),
      commentFixture({ id: 3, path: 'b.ts', sentAt: 1 }),
      commentFixture({ id: 4, path: 'c.ts' }),
    ];
    expect(unsentByPath(list)).toEqual(
      new Map([
        ['a.ts', 2],
        ['c.ts', 1],
      ]),
    );
  });
});

describe('byPath', () => {
  it('path별로 묶고 입력 순서를 지킨다', () => {
    const a1 = commentFixture({ id: 1, path: 'a.ts' });
    const b = commentFixture({ id: 2, path: 'b.ts' });
    const a2 = commentFixture({ id: 3, path: 'a.ts', sentAt: 1 });
    expect(byPath([a1, b, a2])).toEqual(
      new Map([
        ['a.ts', [a1, a2]],
        ['b.ts', [b]],
      ]),
    );
  });
});

describe('firstLine', () => {
  it('첫 줄만, 앞뒤 공백 없이', () => {
    expect(firstLine('  고쳐 주세요  \n둘째 줄')).toBe('고쳐 주세요');
    expect(firstLine('한 줄')).toBe('한 줄');
  });

  it('첫 줄이 비어 있으면 내용이 있는 첫 줄', () => {
    expect(firstLine('\n\n  셋째 줄\n넷째')).toBe('셋째 줄');
  });
});
