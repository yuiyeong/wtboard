import { useEffect, useMemo, useRef, useState, type MouseEvent } from 'react';
import type { CommentItem } from '../api';
import { scrollToFile } from '../lib/anchor';
import {
  commentCounts,
  firstLine,
  headerLabel,
  replyLabel,
  sortForList,
  statusLabel,
  type CommentCounts,
} from '../lib/comments';
import { useCommentActions } from '../useComments';
import { ACTION } from './CommentWidget';

interface Props {
  worktreeId: string;
  comments: CommentItem[];
  /** 지금 파일 목록에 있는 path. 없는 파일의 코멘트는 `목록에 없음`이고 눌러도 스크롤하지 않는다 */
  listedPaths: Set<string>;
  autoAttach: boolean;
}

/** worktree header의 `💬 미전송 a · 응답 b` 버튼. 누르면 코멘트 목록 popover */
export function CommentsPopover({ worktreeId, comments, listedPaths, autoAttach }: Props) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const counts = useMemo(() => commentCounts(comments), [comments]);
  const color = counts.unsent > 0 ? 'text-blue-400' : counts.replies > 0 ? 'text-amber-400' : 'text-gray-500';

  // popover 밖을 누르면 닫는다
  useEffect(() => {
    if (!open) return;
    const onDown = (e: globalThis.MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', onDown);
    return () => document.removeEventListener('mousedown', onDown);
  }, [open]);

  return (
    <div ref={root} className="relative">
      <button onClick={() => setOpen(!open)} className={`rounded px-1.5 py-0.5 text-xs hover:bg-gray-800 ${color}`}>
        {headerLabel(counts)}
      </button>
      {open && (
        <CommentList
          worktreeId={worktreeId}
          comments={comments}
          counts={counts}
          listedPaths={listedPaths}
          autoAttach={autoAttach}
          onJump={(path) => {
            scrollToFile(path);
            setOpen(false);
          }}
        />
      )}
    </div>
  );
}

interface ListProps extends Props {
  counts: CommentCounts;
  onJump: (path: string) => void;
}

/** 열려 있을 때만 mount된다: 미전송 → 전송됨 목록, 그 아래 접힌 "해결된 코멘트 n개" */
function CommentList({ worktreeId, comments, counts, listedPaths, autoAttach, onJump }: ListProps) {
  const { copy, clearSent, resolve, unresolve, resend } = useCommentActions(worktreeId);
  // 정렬이 해결됨을 맨 뒤에 두므로 첫 해결됨에서 자른다
  const { active, resolved } = useMemo(() => {
    const sorted = sortForList(comments);
    const first = sorted.findIndex((c) => c.resolvedAt !== null);
    return first < 0 ? { active: sorted, resolved: [] } : { active: sorted.slice(0, first), resolved: sorted.slice(first) };
  }, [comments]);
  const error = copy.error ?? clearSent.error ?? resolve.error ?? unresolve.error ?? resend.error;
  const rowProps = { listedPaths, onJump, resolve: resolve.mutate, unresolve: unresolve.mutate, resend: resend.mutate };

  return (
    <div className="absolute top-full right-0 z-20 mt-1 w-96 rounded border border-gray-700 bg-gray-900 text-xs shadow-lg">
      {!autoAttach && (
        <p className="border-b border-amber-900 bg-amber-950 px-3 py-2 text-amber-200">
          자동 첨부 꺼짐: 복사해서 Claude Code에 붙여 넣으세요
        </p>
      )}
      {comments.length > 0 && (
        <div className="flex items-center gap-2 border-b border-gray-800 px-3 py-2">
          {counts.unsent > 0 && (
            <button
              onClick={() => copy.mutate()}
              disabled={copy.isPending}
              className="rounded bg-blue-600 px-2 py-0.5 text-white hover:bg-blue-700 disabled:opacity-50"
            >
              마크다운으로 복사
            </button>
          )}
          <span className="flex-1" />
          {counts.sent > 0 && (
            <button
              onClick={() => clearSent.mutate()}
              disabled={clearSent.isPending}
              className={`text-gray-400 ${ACTION}`}
            >
              전송된 코멘트 지우기
            </button>
          )}
        </div>
      )}
      {error && <p className="border-b border-gray-800 px-3 py-2 text-red-400">{String(error)}</p>}
      {comments.length === 0 ? (
        <p className="px-3 py-3 text-gray-400">코멘트가 없습니다</p>
      ) : (
        <div className="max-h-[60vh] overflow-y-auto py-1">
          <ul>
            {active.map((c) => (
              <CommentRow key={c.id} c={c} {...rowProps} />
            ))}
          </ul>
          {resolved.length > 0 && (
            <details className="border-t border-gray-800">
              <summary className="cursor-pointer px-3 py-1.5 text-gray-400">해결된 코멘트 {resolved.length}개</summary>
              <ul>
                {resolved.map((c) => (
                  <CommentRow key={c.id} c={c} {...rowProps} />
                ))}
              </ul>
            </details>
          )}
        </div>
      )}
    </div>
  );
}

interface RowProps {
  c: CommentItem;
  listedPaths: Set<string>;
  onJump: (path: string) => void;
  resolve: (id: number) => void;
  unresolve: (id: number) => void;
  resend: (id: number) => void;
}

/** 목록 한 줄. 누르면 그 파일로 스크롤. 전송됨이면 `응답 #n`과 `해결됨`·`다시 보내기`, 해결됨이면 `되돌리기` */
function CommentRow({ c, listedPaths, onJump, resolve, unresolve, resend }: RowProps) {
  const listed = listedPaths.has(c.path);
  const isResolved = c.resolvedAt !== null;
  const reply = replyLabel(c);
  // row 전체가 "파일로 가기"라서 row 안의 버튼은 그 클릭을 막는다
  const act = (mutate: (id: number) => void) => (e: MouseEvent) => {
    e.stopPropagation();
    mutate(c.id);
  };
  const button = `shrink-0 text-gray-400 ${ACTION}`;

  return (
    <li onClick={() => listed && onJump(c.path)} className={`px-3 py-1.5 ${listed ? 'cursor-pointer hover:bg-gray-800' : ''}`}>
      <span className="flex items-center gap-2">
        <span className="min-w-0 flex-1 truncate font-mono text-gray-300" title={c.path}>
          {c.path}:{c.line}
        </span>
        {/* 삭제된 줄의 코멘트는 옛 줄 번호라 새 줄 번호와 헷갈리지 않게 표시한다 */}
        {c.side === 'old' && (
          <span className="shrink-0 text-gray-400" title="옛 줄 번호입니다">
            삭제된 줄
          </span>
        )}
        {!listed && <span className="shrink-0 text-gray-500">목록에 없음</span>}
        {reply && <span className="shrink-0 text-amber-300">{reply}</span>}
        <span className={`shrink-0 ${c.sentAt === null ? 'text-blue-400' : 'text-gray-500'}`}>{statusLabel(c)}</span>
      </span>
      <span className="flex items-center gap-2">
        <span className={`min-w-0 flex-1 truncate ${c.sentAt === null ? '' : 'text-gray-400'}`}>
          {firstLine(c.body)}
        </span>
        {isResolved ? (
          <button onClick={act(unresolve)} className={button}>
            되돌리기
          </button>
        ) : (
          c.sentAt !== null && (
            <>
              <button onClick={act(resolve)} className={button}>
                해결됨
              </button>
              <button onClick={act(resend)} className={button}>
                다시 보내기
              </button>
            </>
          )
        )}
      </span>
    </li>
  );
}
