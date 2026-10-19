import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import type { Comment } from '../api';
import { statusLabel } from '../lib/comments';

/** 성공하면 resolve, 실패하면 오류로 reject한다. 참조가 안정적이어야 FileDiff memo가 유지된다 */
export type OnUpdateComment = (vars: { id: number; body: string }) => Promise<unknown>;
export type OnDeleteComment = (id: number) => Promise<unknown>;

/**
 * 코멘트 입력 (새 코멘트·수정 공용). Cmd+Enter 저장, Esc 취소.
 * 저장이 실패하면 닫지 않고 아래에 오류를 보인다 (입력한 글을 잃지 않게). 닫는 것은 `onSave`를 넘긴 쪽이 한다
 */
export function CommentInput({
  initial = '',
  onSave,
  onCancel,
}: {
  initial?: string;
  onSave: (body: string) => Promise<unknown>;
  onCancel: () => void;
}) {
  const [body, setBody] = useState(initial);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const empty = body.trim() === '';
  // 열리면 바로 쓸 수 있게 focus하고, 수정이면 커서를 글 끝에 둔다
  const textarea = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    const el = textarea.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, []);

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
        onChange={(e) => setBody(e.target.value)}
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

/** 저장된 코멘트 하나. 미전송은 파란 테두리, 전송됨은 회색. 미전송만 `수정` */
export function SavedComment({
  comment,
  onUpdate,
  onDelete,
}: {
  comment: Comment;
  onUpdate: OnUpdateComment;
  onDelete: OnDeleteComment;
}) {
  const [editing, setEditing] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const unsent = comment.sentAt === null;

  if (editing) {
    return (
      <CommentInput
        initial={comment.body}
        onSave={async (body) => {
          await onUpdate({ id: comment.id, body });
          setEditing(false);
        }}
        onCancel={() => setEditing(false)}
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
          <button onClick={() => setEditing(true)} className="hover:underline">
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
export function CommentThread({
  comments,
  onUpdate,
  onDelete,
}: {
  comments: Comment[];
  onUpdate: OnUpdateComment;
  onDelete: OnDeleteComment;
}) {
  return comments.map((c) => <SavedComment key={c.id} comment={c} onUpdate={onUpdate} onDelete={onDelete} />);
}
