import { useEffect, useMemo, useRef, useState } from 'react';
import type { Comment } from '../api';
import { scrollToFile } from '../lib/anchor';
import { firstLine, sortForList, statusLabel } from '../lib/comments';
import { useCommentActions } from '../useComments';

interface Props {
  worktreeId: string;
  comments: Comment[];
  /** 지금 파일 목록에 있는 path. 없는 파일의 코멘트는 `목록에 없음`이고 눌러도 스크롤하지 않는다 */
  listedPaths: Set<string>;
  autoAttach: boolean;
}

/** worktree header의 `💬 미전송 n` 버튼과 코멘트 목록 popover */
export function CommentsPopover({ worktreeId, comments, listedPaths, autoAttach }: Props) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const { copy, clearSent } = useCommentActions(worktreeId);
  const sorted = useMemo(() => sortForList(comments), [comments]);
  const unsent = comments.filter((c) => c.sentAt === null).length;
  const sent = comments.length - unsent;
  const error = copy.error ?? clearSent.error;

  // popover 밖을 누르면 닫는다
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', onDown);
    return () => document.removeEventListener('mousedown', onDown);
  }, [open]);

  const jump = (c: Comment) => {
    if (!listedPaths.has(c.path)) return;
    scrollToFile(c.path);
    setOpen(false);
  };

  return (
    <div ref={root} className="relative">
      <button
        onClick={() => setOpen(!open)}
        className={`rounded px-1.5 py-0.5 text-xs hover:bg-gray-100 ${unsent > 0 ? 'text-blue-600' : 'text-gray-400'}`}
      >
        {unsent > 0 ? `💬 미전송 ${unsent}` : '💬'}
      </button>
      {open && (
        <div className="absolute top-full right-0 z-20 mt-1 w-96 rounded border border-gray-200 bg-white text-xs shadow-lg">
          {!autoAttach && (
            <p className="border-b border-amber-100 bg-amber-50 px-3 py-2 text-amber-800">
              자동 첨부 꺼짐: 복사해서 Claude Code에 붙여 넣으세요
            </p>
          )}
          {(unsent > 0 || sent > 0) && (
            <div className="flex items-center gap-2 border-b border-gray-100 px-3 py-2">
              {unsent > 0 && (
                <button
                  onClick={() => copy.mutate()}
                  disabled={copy.isPending}
                  className="rounded bg-blue-600 px-2 py-0.5 text-white hover:bg-blue-700 disabled:opacity-50"
                >
                  마크다운으로 복사
                </button>
              )}
              <span className="flex-1" />
              {sent > 0 && (
                <button
                  onClick={() => clearSent.mutate()}
                  disabled={clearSent.isPending}
                  className="text-gray-500 hover:underline disabled:opacity-50"
                >
                  전송된 코멘트 지우기
                </button>
              )}
            </div>
          )}
          {error && <p className="border-b border-gray-100 px-3 py-2 text-red-600">{String(error)}</p>}
          {sorted.length === 0 ? (
            <p className="px-3 py-3 text-gray-500">코멘트가 없습니다</p>
          ) : (
            <ul className="max-h-[60vh] overflow-y-auto py-1">
              {sorted.map((c) => {
                const listed = listedPaths.has(c.path);
                return (
                  <li key={c.id}>
                    <button
                      onClick={() => jump(c)}
                      className={`block w-full px-3 py-1.5 text-left ${listed ? 'hover:bg-gray-100' : 'cursor-default'}`}
                    >
                      <span className="flex items-center gap-2">
                        <span className="min-w-0 flex-1 truncate font-mono text-gray-700" title={c.path}>
                          {c.path}:{c.line}
                        </span>
                        {!listed && <span className="shrink-0 text-gray-400">목록에 없음</span>}
                        <span className={`shrink-0 ${c.sentAt === null ? 'text-blue-600' : 'text-gray-400'}`}>
                          {statusLabel(c)}
                        </span>
                      </span>
                      <span className={`block truncate ${c.sentAt === null ? 'text-gray-800' : 'text-gray-500'}`}>
                        {firstLine(c.body)}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}
