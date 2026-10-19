import { beforeEach, describe, expect, it } from 'vitest';
import {
  changeDraft,
  closeDraft,
  draftKey,
  focusedInput,
  isWriting,
  openDraft,
  setEdit,
  startEdit,
  useDraftStore,
} from './drafts';
import type { Anchor } from './lib/comments';

const a: Anchor = { side: 'new', line: 1, snippet: 'ctx' };
const b: Anchor = { side: 'new', line: 3, snippet: 'tail' };
const old: Anchor = { side: 'old', line: 2, snippet: 'old line' };
const read = () => useDraftStore.getState();

beforeEach(() => {
  useDraftStore.setState({ drafts: {}, edits: {} });
  focusedInput.current = null;
});

describe('draft store', () => {
  it('FileDiff 밖에 있어서 다시 mount되어도 key로 다시 읽으면 쓰던 글이 남아 있다', () => {
    const key = draftKey('w', 'a.ts');
    openDraft(key, a, 'session');
    changeDraft(key, '고쳐 주세요');
    // 세션이 바뀌어 FileDiff가 새로 mount된 것: 같은 worktree·path로 다시 읽는다
    expect(read().drafts[draftKey('w', 'a.ts')]).toEqual({ anchor: a, body: '고쳐 주세요', view: 'session' });
  });

  it('worktree와 path마다 따로 둔다 (worktree id에 `:`가 있어도 섞이지 않는다)', () => {
    openDraft(draftKey('w1', 'a.ts'), a, 'branch');
    changeDraft(draftKey('w1', 'a.ts'), 'w1');
    openDraft(draftKey('w2', 'a.ts'), b, 'branch');
    expect(read().drafts[draftKey('w1', 'a.ts')].body).toBe('w1');
    expect(read().drafts[draftKey('w2', 'a.ts')]).toEqual({ anchor: b, body: '', view: 'branch' });
    expect(read().drafts[draftKey('w1', 'b.ts')]).toBeUndefined();
    expect(draftKey('/x:y', 'z')).not.toBe(draftKey('/x', 'y:z'));
  });

  it('다른 줄의 `+`를 눌러도 쓰던 글은 그대로 두고, 연 입력이 focus를 갖는다', () => {
    const key = draftKey('w', 'a.ts');
    openDraft(key, a, 'branch');
    changeDraft(key, '쓰는 중');
    openDraft(key, b, 'turn');
    expect(read().drafts[key]).toEqual({ anchor: a, body: '쓰는 중', view: 'branch' });
    expect(focusedInput.current).toBe(`draft:${key}`);
  });

  it('old side anchor는 연 view를 함께 둔다 (다른 view에서는 위치가 바뀐 묶음에 보인다)', () => {
    const key = draftKey('w', 'a.ts');
    openDraft(key, old, 'session');
    expect(read().drafts[key].view).toBe('session');
  });

  it('저장하거나 취소하면 (closeDraft) 그 파일의 입력만 지운다', () => {
    openDraft(draftKey('w', 'a.ts'), a, 'branch');
    openDraft(draftKey('w', 'b.ts'), a, 'branch');
    closeDraft(draftKey('w', 'a.ts'));
    expect(read().drafts[draftKey('w', 'a.ts')]).toBeUndefined();
    expect(read().drafts[draftKey('w', 'b.ts')]).toBeDefined();
  });

  it('입력이 없을 때 changeDraft는 아무것도 하지 않는다', () => {
    changeDraft(draftKey('w', 'a.ts'), 'x');
    expect(read().drafts).toEqual({});
  });

  it('수정 중인 본문은 comment id로 두고 null이면 끝낸다', () => {
    startEdit(7, '처음');
    expect(focusedInput.current).toBe('edit:7');
    setEdit(7, '고침');
    setEdit(8, '다른 코멘트');
    expect(read().edits).toEqual({ 7: '고침', 8: '다른 코멘트' });
    setEdit(7, null);
    expect(read().edits).toEqual({ 8: '다른 코멘트' });
  });
});

describe('isWriting (접힌 파일 header의 `작성 중`)', () => {
  const key = draftKey('w', 'a.ts');

  it('내용이 있는 새 입력이나 이 파일 코멘트의 수정이 있으면 true', () => {
    expect(isWriting(read(), key, [])).toBe(false);
    openDraft(key, a, 'branch');
    expect(isWriting(read(), key, [])).toBe(false); // 빈 입력은 접어도 잃을 것이 없다
    changeDraft(key, 'x');
    expect(isWriting(read(), key, [])).toBe(true);
    closeDraft(key);
    startEdit(3, '수정');
    expect(isWriting(read(), key, [{ id: 1 }, { id: 2 }])).toBe(false); // 다른 파일의 코멘트
    expect(isWriting(read(), key, [{ id: 3 }])).toBe(true);
  });
});
