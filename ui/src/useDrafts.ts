import { useCallback, useMemo, useRef, useState, type RefObject } from 'react';
import { nextDraft, type Anchor, type Draft } from './lib/comments';

/**
 * 파일 diff 하나에서 쓰고 있는 글 (새 코멘트 입력 하나와 수정 중인 코멘트들).
 * 입력 widget은 diff 안에 그려져서 diff가 실시간으로 바뀌면 다시 mount되므로, 쓰던 글은 FileDiff에 둔다
 */
export interface Drafts {
  /** 새 코멘트 입력. 파일마다 하나 */
  draft: Draft | null;
  /** 줄의 `+`. 쓰던 글이 있으면 버리지 않는다 */
  open: (anchor: Anchor) => void;
  change: (body: string) => void;
  close: () => void;
  /** 수정 중인 코멘트 id → 고치고 있는 본문 */
  edits: ReadonlyMap<number, string>;
  /** `수정`. 고칠 본문을 지금 본문으로 시작한다 */
  startEdit: (id: number, body: string) => void;
  /** null이면 수정을 끝낸다 */
  setEdit: (id: number, body: string | null) => void;
  /**
   * focus를 가질 입력 (`draft` 또는 `edit:<id>`). 다시 mount된 입력이 다른 입력의 focus를 뺏지 않고,
   * 쓰던 입력은 focus를 되찾게 한다
   */
  focused: RefObject<string | null>;
}

export function useDrafts(): Drafts {
  const [draft, setDraft] = useState<Draft | null>(null);
  const [edits, setEdits] = useState<ReadonlyMap<number, string>>(() => new Map());
  const focused = useRef<string | null>(null);
  const open = useCallback((anchor: Anchor) => {
    focused.current = 'draft';
    setDraft((current) => nextDraft(current, anchor));
  }, []);
  const change = useCallback((body: string) => setDraft((current) => current && { ...current, body }), []);
  const close = useCallback(() => setDraft(null), []);
  const setEdit = useCallback(
    (id: number, body: string | null) =>
      setEdits((prev) => {
        const next = new Map(prev);
        if (body === null) next.delete(id);
        else next.set(id, body);
        return next;
      }),
    [],
  );
  const startEdit = useCallback(
    (id: number, body: string) => {
      focused.current = `edit:${id}`;
      setEdit(id, body);
    },
    [setEdit],
  );
  return useMemo(
    () => ({ draft, open, change, close, edits, startEdit, setEdit, focused }),
    [draft, open, change, close, edits, startEdit, setEdit],
  );
}
