import { useEffect, useRef, useState, type KeyboardEvent, type RefObject } from 'react';
import type { Comment } from '../api';
import { statusLabel } from '../lib/comments';
import type { Drafts } from '../useDrafts';

/** 성공하면 resolve, 실패하면 오류로 reject한다. 참조가 안정적이어야 FileDiff memo가 유지된다 */
export type OnUpdateComment = (vars: { id: number; body: string }) => Promise<unknown>;
export type OnDeleteComment = (id: number) => Promise<unknown>;

/**
 * 코멘트 입력 (새 코멘트·수정 공용). Cmd+Enter 저장, Esc 취소.
 * 본문은 부르는 쪽(`Drafts`)이 갖는다. diff가 바뀌어 이 widget이 다시 mount되어도 쓰던 글이 남게 하려고.
 * 저장이 실패하면 닫지 않고 아래에 오류를 보인다. 닫는 것은 `onSave`를 넘긴 쪽이 한다
 */
export function CommentInput({
  body,
  onChange,
  onSave,
  onCancel,
  focusId,
  focused,
}: {
  body: string;
  onChange: (body: string) => void;
  onSave: (body: string) => Promise<unknown>;
  onCancel: () => void;
  /** `Drafts.focused`에서 이 입력을 가리키는 값 */
  focusId: string;
  focused: RefObject<string | null>;
}) {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const empty = body.trim() === '';
  // 새로 열었거나 쓰던 중에 diff가 바뀌어 다시 mount되면 focus를 가져오고 커서를 글 끝에 둔다.
  // 다른 입력을 쓰는 중이면 focus를 뺏지 않는다
  const textarea = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    const el = textarea.current;
    if (!el || focused.current !== focusId) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, [focused, focusId]);

  const save = async () => {
    if (saving || empty) return;
    setSaving(true);
    setError(null);
    try {
      await onSave(body);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };
  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing) return; // 한글 조합 중의 Enter·Esc는 조합을 끝내는 키다
    if (e.key === 'Enter' && e.metaKey) {
      e.preventDefault();
      save();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onCancel();
    }
  };

  return (
    <div className="rounded border border-blue-300 bg-white p-2">
      <textarea
        ref={textarea}
        rows={3}
        value={body}
        onChange={(e) => onChange(e.target.value)}
        onFocus={() => (focused.current = focusId)}
        onBlur={(e) => {
          // 다시 mount되느라 DOM에서 빠질 때가 아니라 사용자가 다른 곳을 눌렀을 때만 놓는다
          if (e.currentTarget.isConnected && focused.current === focusId) focused.current = null;
        }}
        onKeyDown={onKeyDown}
        className="block w-full resize-y rounded border border-gray-300 p-1.5 text-sm focus:border-blue-400 focus:outline-none"
      />
      <div className="mt-1.5 flex items-center justify-end gap-2 text-xs">
        <button onClick={onCancel} className="rounded border border-gray-300 px-2 py-0.5 hover:bg-gray-100">
          취소
        </button>
        <button
          onClick={save}
          disabled={saving || empty}
          className="rounded bg-blue-600 px-2 py-0.5 text-white hover:bg-blue-700 disabled:opacity-50"
        >
          저장
        </button>
      </div>
      {error && <p className="mt-1 text-xs text-red-600">{error}</p>}
    </div>
  );
}

interface SavedProps {
  onUpdate: OnUpdateComment;
  onDelete: OnDeleteComment;
  /** 수정 중인 본문은 여기에 둔다 */
  drafts: Drafts;
}

/** 저장된 코멘트 하나. 미전송은 파란 테두리, 전송됨은 회색. 미전송만 `수정` */
export function SavedComment({ comment, onUpdate, onDelete, drafts }: SavedProps & { comment: Comment }) {
  const editing = drafts.edits.get(comment.id);
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const unsent = comment.sentAt === null;

  if (editing !== undefined) {
    return (
      <CommentInput
        body={editing}
        focusId={`edit:${comment.id}`}
        focused={drafts.focused}
        onChange={(body) => drafts.setEdit(comment.id, body)}
        onSave={async (body) => {
          await onUpdate({ id: comment.id, body });
          drafts.setEdit(comment.id, null);
        }}
        onCancel={() => drafts.setEdit(comment.id, null)}
      />
    );
  }

  const remove = async () => {
    setDeleting(true);
    setError(null);
    try {
      await onDelete(comment.id);
    } catch (e) {
      setError(String(e));
      setDeleting(false);
    }
  };

  return (
    <div className={`rounded border bg-white ${unsent ? 'border-blue-300' : 'border-gray-200'}`}>
      <div
        className={`flex items-center gap-3 border-b px-2 py-1 text-xs ${
          unsent ? 'border-blue-100 bg-blue-50 text-blue-700' : 'border-gray-100 bg-gray-50 text-gray-500'
        }`}
      >
        <span className="flex-1">{statusLabel(comment)}</span>
        {unsent && (
          <button onClick={() => drafts.startEdit(comment.id, comment.body)} className="hover:underline">
            수정
          </button>
        )}
        <button onClick={remove} disabled={deleting} className="hover:underline disabled:opacity-50">
          삭제
        </button>
      </div>
      <p className={`px-2 py-1.5 text-sm whitespace-pre-wrap ${unsent ? '' : 'text-gray-500'}`}>{comment.body}</p>
      {error && <p className="px-2 pb-1.5 text-xs text-red-600">{error}</p>}
    </div>
  );
}

/** 한 줄 아래에 보이는 코멘트들 */
export function CommentThread({ comments, ...props }: SavedProps & { comments: Comment[] }) {
  return comments.map((c) => <SavedComment key={c.id} comment={c} {...props} />);
}
