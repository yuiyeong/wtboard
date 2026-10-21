import { memo, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Diff, getChangeKey, Hunk, parseDiff, type ChangeData, type RenderGutter } from 'react-diff-view';
import { getPatch, type ChangedFile, type CommentItem, type NewComment, type PatchBody, type View } from '../api';
import { fileAnchorId } from '../lib/anchor';
import { changeDraft, closeDraft, draftKey, isWriting, openDraft, useDraftStore } from '../drafts';
import { anchorOf, placeComments, placeDraft, type Anchor, type Draft } from '../lib/comments';
import { tokenizeHunks } from '../lib/highlight';
import { languageForPath } from '../lib/language';
import { patchKey } from '../lib/sessions';
import { useInView } from '../lib/useInView';
import { initiallyOpen, nextOpen } from '../lib/viewed';
import type { ViewType } from '../store';
import { CommentInput, CommentThread, SavedComment, type SavedProps } from './CommentWidget';
import { LineCounts } from './FileTree';

/** 성공하면 resolve, 실패하면 오류로 reject한다 */
export type OnAddComment = (comment: NewComment) => Promise<unknown>;

/** 코멘트 표시·입력에 필요한 것. 모두 참조가 안정적이어야 memo가 유지된다 */
interface CommentProps extends SavedProps {
  /** 이 파일의 코멘트 (해결됨 제외) */
  comments: CommentItem[];
  onAdd: OnAddComment;
}

/** `SavedProps`만 골라낸다 (CommentThread·OutdatedComments에 넘길 것) */
function savedProps(p: SavedProps): SavedProps {
  const { onUpdate, onDelete, onResolve, onResend, replyTargetOf, onOpenReply } = p;
  return { onUpdate, onDelete, onResolve, onResend, replyTargetOf, onOpenReply };
}

interface Props extends CommentProps {
  worktreeId: string;
  file: ChangedFile;
  view: View;
  /** branch view에서 null */
  sessionId: string | null;
  viewType: ViewType;
  /** 참조가 안정적이어야 memo가 유지된다 */
  onSetViewed: (file: ChangedFile, viewed: boolean, hash: string | null) => void;
}

// 레포 목록이 갱신될 때마다(파일 event마다) 화면 전체가 다시 그려지므로, 바뀐 파일만 다시 그린다
export const FileDiff = memo(function FileDiff({
  worktreeId,
  file,
  view,
  sessionId,
  viewType,
  onSetViewed,
  ...commentProps
}: Props) {
  const [ref, inView] = useInView<HTMLElement>();
  // 생성 파일과 본 파일은 접힌 채로 시작한다. 접혀 있으면 patch를 계산하지 않는다
  const [open, setOpen] = useState(() => initiallyOpen(file));
  // 봤음 상태가 바뀌면 펼침을 맞춘다 (체크하면 접고, 해제하거나 다시 바뀌면 펼친다)
  const prevViewed = useRef(file.viewed);
  useEffect(() => {
    const prev = prevViewed.current;
    prevViewed.current = file.viewed;
    setOpen((open) => nextOpen(prev, file.viewed, open));
  }, [file.viewed]);
  const patch = useQuery({
    queryKey: patchKey(worktreeId, file.path, view, sessionId),
    queryFn: () => getPatch(worktreeId, view, file.path, sessionId),
    enabled: inView && open, // 화면에 보이고 펼쳐져 있을 때만 계산
  });
  const unsent = commentProps.comments.filter((c) => c.sentAt === null).length;
  // 쓰던 글은 FileDiff 밖(store)에 있어서 patch·세션·view·worktree가 바뀌어 다시 mount되어도 남는다
  const dkey = draftKey(worktreeId, file.path);
  const draft = useDraftStore((s) => s.drafts[dkey]);
  const writing = useDraftStore((s) => isWriting(s, dkey, commentProps.comments));

  return (
    <section ref={ref} id={fileAnchorId(file.path)} className="mb-4 rounded border border-gray-200">
      <header
        className="sticky top-0 z-10 flex cursor-pointer items-center gap-3 border-b border-gray-200 bg-gray-50 px-3 py-2 text-sm"
        onClick={() => setOpen(!open)}
      >
        <span className="w-3 text-xs text-gray-500">{open ? '▾' : '▸'}</span>
        <span className="flex-1 truncate font-mono">{file.path}</span>
        {file.viewed === 'changed' && (
          <span className="rounded bg-amber-100 px-1.5 text-xs text-amber-700">다시 바뀜</span>
        )}
        {file.uncertain && (
          <span
            className="text-xs text-gray-400"
            title="세션 전 내용을 정확히 알 수 없습니다. 세션 시작 commit 또는 빈 파일과 비교합니다"
          >
            원본 불확실
          </span>
        )}
        {file.collapsed && !open && <span className="text-xs text-gray-400">생성 파일</span>}
        {!open && writing && <span className="text-xs text-blue-600">작성 중</span>}
        {unsent > 0 && (
          <span className="text-xs text-blue-600" title={`미전송 코멘트 ${unsent}개`}>
            💬 {unsent}
          </span>
        )}
        <LineCounts file={file} />
        {/* 봤음은 branch view에만 있다. checkbox 클릭이 header의 접기 toggle로 번지지 않게 막는다 */}
        {view === 'branch' && (
          <label className="flex items-center gap-1 text-xs text-gray-600" onClick={(e) => e.stopPropagation()}>
            <input
              type="checkbox"
              checked={file.viewed === 'viewed'}
              onChange={(e) => onSetViewed(file, e.target.checked, patch.data?.hash ?? null)}
            />
            봤음
          </label>
        )}
      </header>
      {/* 다시 읽기가 실패해도 이전 diff(와 그 안에서 쓰던 글)는 그대로 두고 오류만 위에 보인다 */}
      {open && patch.error && <p className="p-3 text-sm text-red-600">{String(patch.error)}</p>}
      {open &&
        (patch.data ? (
          <PatchView
            body={patch.data.body}
            viewType={viewType}
            path={file.path}
            view={view}
            draft={draft}
            draftKey={dkey}
            {...commentProps}
          />
        ) : (
          !patch.error && <div className="h-20" />
        ))}
    </section>
  );
});

interface PatchViewProps extends CommentProps {
  body: PatchBody;
  viewType: ViewType;
  path: string;
  view: View;
  /** 이 파일에서 쓰고 있는 새 코멘트 */
  draft: Draft | undefined;
  draftKey: string;
}

function PatchView({ body, viewType, comments, ...props }: PatchViewProps) {
  if (body.kind === 'text') return <TextDiff diff={body.diff} viewType={viewType} comments={comments} {...props} />;
  const label = body.kind === 'binary' ? '바이너리 파일' : '1MB 초과 파일';
  return (
    <>
      {/* 줄이 없으니 코멘트와 쓰던 글은 모두 위치가 바뀐 묶음에 보인다 */}
      <OutdatedComments {...props} outdated={comments} draftOutdated />
      <p className="p-3 text-sm text-gray-500">
        {label} 변경됨 ({formatSize(body.oldSize)} → {formatSize(body.newSize)})
      </p>
    </>
  );
}

const NO_CHANGES: ChangeData[] = [];

function TextDiff({
  diff,
  viewType,
  path,
  view,
  comments,
  onAdd,
  draft,
  draftKey,
  ...rest
}: Omit<PatchViewProps, 'body'> & { diff: string }) {
  const { onUpdate, onDelete, onResolve, onResend, replyTargetOf, onOpenReply } = rest;
  // 참조가 안정적인 handler만 모아 widget memo가 유지되게 한다
  const saved = useMemo(
    () => ({ onUpdate, onDelete, onResolve, onResend, replyTargetOf, onOpenReply }),
    [onUpdate, onDelete, onResolve, onResend, replyTargetOf, onOpenReply],
  );
  const commentProps = { draft, draftKey, onAdd, path, ...saved };
  const file = useMemo(() => parseDiff(diff)[0], [diff]);
  const tokens = useMemo(() => (file ? tokenizeHunks(file.hunks, languageForPath(path)) : undefined), [file, path]);
  const changes = useMemo(() => file?.hunks.flatMap((h) => h.changes) ?? NO_CHANGES, [file]);
  const placed = useMemo(() => placeComments(comments, changes, view), [comments, changes, view]);
  // 쓰던 입력은 change key가 아니라 줄 (side, line)과 내용으로 매번 다시 찾는다
  const draftAt = draft ? placeDraft(draft, changes, view) : null;

  // unified는 한 줄에 gutter가 둘이라 new 쪽에만 `+`를 둔다. split은 누른 쪽 change에 단다
  const renderGutter = useCallback<RenderGutter>(
    ({ change, side, inHoverState, renderDefault }) => (
      <>
        {inHoverState && (viewType === 'split' || side === 'new') && (
          <button
            title="코멘트 달기"
            className="float-left -ml-[0.5ch] w-4 rounded bg-blue-600 text-xs leading-4 text-white hover:bg-blue-700"
            onClick={() => openDraft(draftKey, anchorOf(change, side), view)}
          >
            +
          </button>
        )}
        {renderDefault()}
      </>
    ),
    [viewType, draftKey, view],
  );

  const widgets = useMemo(() => {
    const keys = new Set(placed.byChangeKey.keys());
    if (draftAt) keys.add(draftAt);
    const out: Record<string, ReactNode> = {};
    for (const key of keys) {
      const list = placed.byChangeKey.get(key);
      out[key] = (
        <div className="space-y-2 bg-gray-50 px-3 py-2 font-sans">
          {list && <CommentThread comments={list} {...saved} />}
          {key === draftAt && draft && <DraftInput draft={draft} draftKey={draftKey} onAdd={onAdd} path={path} />}
        </div>
      );
    }
    return out;
  }, [placed, draftAt, draft, draftKey, path, onAdd, saved]);

  if (!file || file.hunks.length === 0) {
    return (
      <>
        <OutdatedComments {...commentProps} outdated={comments} draftOutdated />
        <p className="p-3 text-sm text-gray-500">내용 변경 없음 (빈 파일 또는 권한 변경)</p>
      </>
    );
  }
  return (
    <>
      <OutdatedComments {...commentProps} outdated={placed.outdated} draftOutdated={draftAt === null} />
      <Diff
        viewType={viewType}
        diffType={file.type}
        hunks={file.hunks}
        tokens={tokens}
        widgets={widgets}
        renderGutter={renderGutter}
      >
        {(hunks) => hunks.map((hunk) => <Hunk key={hunk.content} hunk={hunk} />)}
      </Diff>
    </>
  );
}

type DraftProps = Pick<PatchViewProps, 'draftKey' | 'onAdd' | 'path'> & { draft: Draft };

/** 새 코멘트 입력. 본문은 store에 있어서 다시 mount되어도 남는다. 줄 번호가 연 view 기준이라 그 view로 저장한다 */
function DraftInput({ draft, draftKey, onAdd, path }: DraftProps) {
  const { anchor, view } = draft;
  return (
    <CommentInput
      body={draft.body}
      focusId={`draft:${draftKey}`}
      onChange={(body) => changeDraft(draftKey, body)}
      onSave={async (body) => {
        await onAdd({ path, side: anchor.side, line: anchor.line, view, snippet: anchor.snippet, body });
        closeDraft(draftKey);
      }}
      onCancel={() => closeDraft(draftKey)}
    />
  );
}

/** 줄 내용과 함께 보이는 위치 표시 (`삭제된 줄 3: ...`) */
function LineLabel({ side, line, snippet }: Anchor) {
  return (
    <p className="mb-1 truncate font-mono text-xs text-gray-500" title={snippet}>
      {side === 'old' ? '삭제된 줄 ' : ''}
      {line}: {snippet}
    </p>
  );
}

/**
 * 지금 diff에 맞는 줄이 없는 코멘트. 단 줄의 내용(snippet)과 함께 diff 위에 보인다.
 * `draftOutdated`면 쓰던 입력도 여기에 둔다 (그 줄이 바뀌었거나 사라졌을 때)
 */
function OutdatedComments({
  outdated,
  draftOutdated,
  ...props
}: Omit<DraftProps, 'draft'> &
  SavedProps & {
    draft: Draft | undefined;
    outdated: CommentItem[];
    draftOutdated: boolean;
  }) {
  const draft = draftOutdated ? props.draft : undefined;
  const count = outdated.length + (draft ? 1 : 0);
  if (count === 0) return null;
  return (
    <details open className="border-b border-gray-200 bg-gray-50 px-3 py-2 text-sm">
      <summary className="cursor-pointer text-xs text-gray-600">위치가 바뀐 코멘트 {count}개</summary>
      <ul className="mt-2 space-y-3">
        {outdated.map((c) => (
          <li key={c.id}>
            <LineLabel {...c} />
            <SavedComment comment={c} {...savedProps(props)} />
          </li>
        ))}
        {draft && (
          <li>
            <LineLabel {...draft.anchor} />
            <DraftInput {...props} draft={draft} />
          </li>
        )}
      </ul>
    </details>
  );
}

function formatSize(n: number | null) {
  if (n === null) return '없음';
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
