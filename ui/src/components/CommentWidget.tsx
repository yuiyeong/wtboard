import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import type { CommentItem, NewComment } from '../api';
import { focusedInput, setEdit, startEdit, useDraftStore } from '../drafts';
import { statusLabel, type ReplyTarget } from '../lib/comments';

/** 성공하면 resolve, 실패하면 오류로 reject한다 */
export type OnAddComment = (comment: NewComment) => Promise<unknown>;
export type OnUpdateComment = (vars: { id: number; body: string }) => Promise<unknown>;
export type OnCommentAction = (id: number) => Promise<unknown>;

/** 코멘트에 할 수 있는 일 전부. 하나의 객체로 내려보내며, 참조가 안정적이어야 FileDiff memo가 유지된다 */
export interface CommentActions {
  onAdd: OnAddComment;
  onUpdate: OnUpdateComment;
  onDelete: OnCommentAction;
  /** 전송됨 → 해결됨 */
  onResolve: OnCommentAction;
  /** 전송됨 → 미전송 (다음 prompt에 다시 붙는다) */
  onResend: OnCommentAction;
  /** 전송됨 코멘트의 `응답 #n 보기` 대상. 없으면 버튼을 두지 않는다 */
  replyTargetOf: (c: CommentItem) => ReplyTarget | null;
  onOpenReply: (c: CommentItem, target: ReplyTarget) => void;
}

/** 글자만 있는 작은 action 버튼 */
export const ACTION = 'hover:underline disabled:opacity-50';

/**
 * 코멘트 입력 (새 코멘트·수정 공용). Cmd+Enter 저장, Esc 취소.
 * 본문은 `useDraftStore`에 있다. diff가 바뀌어 이 widget이 다시 mount되어도 쓰던 글이 남게 하려고.
 * 저장이 실패하면 닫지 않고 아래에 오류를 보인다. 닫는 것은 `onSave`를 넘긴 쪽이 한다
 */
export function CommentInput({
  body,
  onChange,
  onSave,
  onCancel,
  focusId,
}: {
  body: string;
  onChange: (body: string) => void;
  onSave: (body: string) => Promise<unknown>;
  onCancel: () => void;
  /** `focusedInput`에서 이 입력을 가리키는 값 */
  focusId: string;
}) {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const empty = body.trim() === '';
  // 새로 열었거나 쓰던 중에 diff가 바뀌어 다시 mount되면 focus를 가져오고 커서를 글 끝에 둔다.
  // 다른 입력을 쓰는 중이면 focus를 뺏지 않는다
  const textarea = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    const el = textarea.current;
    if (!el || focusedInput.current !== focusId) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, [focusId]);

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
        onFocus={() => (focusedInput.current = focusId)}
        onBlur={(e) => {
          // 다시 mount되느라 DOM에서 빠질 때가 아니라 사용자가 다른 곳을 눌렀을 때만 놓는다
          if (e.currentTarget.isConnected && focusedInput.current === focusId) focusedInput.current = null;
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

/**
 * 저장된 코멘트 하나. 미전송은 파란 테두리에 `수정`, 전송됨은 회색에 `다시 보내기`·`해결됨`과 응답 버튼.
 * 해결됨은 diff에 오지 않는다 (`openComments`가 거른다)
 */
export function SavedComment({ comment, actions }: { comment: CommentItem; actions: CommentActions }) {
  const { onUpdate, onDelete, onResolve, onResend, replyTargetOf, onOpenReply } = actions;
  const editing = useDraftStore((s) => s.edits[comment.id]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const unsent = comment.sentAt === null;
  const reply = replyTargetOf(comment);

  if (editing !== undefined) {
    return (
      <CommentInput
        body={editing}
        focusId={`edit:${comment.id}`}
        onChange={(body) => setEdit(comment.id, body)}
        onSave={async (body) => {
          await onUpdate({ id: comment.id, body });
          setEdit(comment.id, null);
        }}
        onCancel={() => setEdit(comment.id, null)}
      />
    );
  }

  // 성공하면 목록이 다시 읽혀 이 widget이 바뀌거나 사라진다. 다시 보내기는 같은 자리에 남으므로 busy를 푼다
  const run = async (action: OnCommentAction) => {
    setBusy(true);
    setError(null);
    try {
      await action(comment.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
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
        {reply && (
          <button onClick={() => onOpenReply(comment, reply)} className="text-amber-700 hover:underline">
            {reply.label}
          </button>
        )}
        {unsent ? (
          <button onClick={() => startEdit(comment.id, comment.body)} className={ACTION}>
            수정
          </button>
        ) : (
          <>
            <button onClick={() => run(onResend)} disabled={busy} className={ACTION}>
              다시 보내기
            </button>
            <button onClick={() => run(onResolve)} disabled={busy} className={ACTION}>
              해결됨
            </button>
          </>
        )}
        <button onClick={() => run(onDelete)} disabled={busy} className={ACTION}>
          삭제
        </button>
      </div>
      <p className={`px-2 py-1.5 text-sm whitespace-pre-wrap ${unsent ? '' : 'text-gray-500'}`}>{comment.body}</p>
      {error && <p className="px-2 pb-1.5 text-xs text-red-600">{error}</p>}
    </div>
  );
}

/** 한 줄 아래에 보이는 코멘트들 */
export function CommentThread({ comments, actions }: { comments: CommentItem[]; actions: CommentActions }) {
  return comments.map((c) => <SavedComment key={c.id} comment={c} actions={actions} />);
}
