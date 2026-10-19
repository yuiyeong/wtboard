import { create } from 'zustand';
import type { Comment, View } from './api';
import { nextDraft, type Anchor, type Draft } from './lib/comments';

/**
 * 쓰고 있는 코멘트 (파일마다 새 코멘트 입력 하나와, 수정 중인 코멘트들). 디스크에는 저장하지 않는다.
 * FileDiff 밖에 두어서 diff가 실시간으로 바뀌거나, 세션·view·worktree가 바뀌어 FileDiff가 다시 mount되거나,
 * 파일이 목록에서 잠시 빠져도 쓰던 글이 남는다
 */
interface DraftState {
  /** `draftKey(worktreeId, path)` → 새 코멘트 입력 */
  drafts: Record<string, Draft>;
  /** comment id → 고치고 있는 본문 */
  edits: Record<number, string>;
}

export const useDraftStore = create<DraftState>()(() => ({ drafts: {}, edits: {} }));

/** worktree id는 경로라 `:`가 들어갈 수 있으므로 경로에 없는 NUL로 잇는다 */
export const draftKey = (worktreeId: string, path: string) => `${worktreeId}\0${path}`;

/**
 * focus를 가질 입력 (`draft:<key>` 또는 `edit:<id>`). 다시 mount된 입력이 다른 입력의 focus를 뺏지 않고,
 * 쓰던 입력은 focus를 되찾게 한다. 그리는 데 쓰지 않으므로 state가 아니다
 */
export const focusedInput: { current: string | null } = { current: null };

/** 줄의 `+`. 쓰던 글이 있으면 버리지 않는다 */
export function openDraft(key: string, anchor: Anchor, view: View) {
  focusedInput.current = `draft:${key}`;
  useDraftStore.setState((s) => ({ drafts: { ...s.drafts, [key]: nextDraft(s.drafts[key], anchor, view) } }));
}

export function changeDraft(key: string, body: string) {
  useDraftStore.setState((s) => {
    const draft = s.drafts[key];
    return draft ? { drafts: { ...s.drafts, [key]: { ...draft, body } } } : s;
  });
}

/** 저장했거나 취소했을 때 */
export function closeDraft(key: string) {
  useDraftStore.setState((s) => {
    const { [key]: _closed, ...drafts } = s.drafts;
    return { drafts };
  });
}

/** `수정`. 고칠 본문을 지금 본문으로 시작한다 */
export function startEdit(id: number, body: string) {
  focusedInput.current = `edit:${id}`;
  setEdit(id, body);
}

/** null이면 수정을 끝낸다 */
export function setEdit(id: number, body: string | null) {
  useDraftStore.setState((s) => {
    const { [id]: _previous, ...edits } = s.edits;
    return { edits: body === null ? edits : { ...edits, [id]: body } };
  });
}

/** 파일을 접으면 보이지 않는 쓰던 글이 있는지 (내용이 있는 새 입력, 또는 이 파일 코멘트의 수정) */
export function isWriting(state: DraftState, key: string, comments: Pick<Comment, 'id'>[]): boolean {
  const draft = state.drafts[key];
  return (draft !== undefined && draft.body.trim() !== '') || comments.some((c) => c.id in state.edits);
}
