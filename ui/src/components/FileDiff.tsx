import { memo, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Diff, getChangeKey, Hunk, parseDiff, type ChangeData, type RenderGutter } from 'react-diff-view';
import { getPatch, type ChangedFile, type Comment, type NewComment, type PatchBody, type View } from '../api';
import { fileAnchorId } from '../lib/anchor';
import { anchorOf, placeComments, placeDraft, type Anchor } from '../lib/comments';
import { tokenizeHunks } from '../lib/highlight';
import { languageForPath } from '../lib/language';
import { patchKey } from '../lib/sessions';
import { useInView } from '../lib/useInView';
import { initiallyOpen, nextOpen } from '../lib/viewed';
import type { ViewType } from '../store';
import { useDrafts, type Drafts } from '../useDrafts';
import { CommentInput, CommentThread, SavedComment, type OnDeleteComment, type OnUpdateComment } from './CommentWidget';
import { LineCounts } from './FileTree';

/** 성공하면 resolve, 실패하면 오류로 reject한다 */
export type OnAddComment = (comment: NewComment) => Promise<unknown>;

/** 코멘트 표시·입력에 필요한 것. 모두 참조가 안정적이어야 memo가 유지된다 */
interface CommentProps {
  /** 이 파일의 코멘트 */
  comments: Comment[];
  onAdd: OnAddComment;
  onUpdate: OnUpdateComment;
  onDelete: OnDeleteComment;
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
  // 쓰던 글은 patch가 바뀌거나 접었다 펴도 남게 여기에 둔다
  const drafts = useDrafts();

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
            drafts={drafts}
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
  drafts: Drafts;
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
  onUpdate,
  onDelete,
  drafts,
}: Omit<PatchViewProps, 'body'> & { diff: string }) {
  const commentProps = { drafts, onAdd, onUpdate, onDelete, path, view };
  const file = useMemo(() => parseDiff(diff)[0], [diff]);
  const tokens = useMemo(() => (file ? tokenizeHunks(file.hunks, languageForPath(path)) : undefined), [file, path]);
  const changes = useMemo(() => file?.hunks.flatMap((h) => h.changes) ?? NO_CHANGES, [file]);
  const placed = useMemo(() => placeComments(comments, changes, view), [comments, changes, view]);
  // 쓰던 입력은 change key가 아니라 줄 (side, line)과 내용으로 매번 다시 찾는다
  const draftKey = drafts.draft && placeDraft(drafts.draft.anchor, changes);

  // unified는 한 줄에 gutter가 둘이라 new 쪽에만 `+`를 둔다. split은 누른 쪽 change에 단다
  const { open } = drafts;
  const renderGutter = useCallback<RenderGutter>(
    ({ change, side, inHoverState, renderDefault }) => (
      <>
        {inHoverState && (viewType === 'split' || side === 'new') && (
          <button
            title="코멘트 달기"
            className="float-left -ml-[0.5ch] w-4 rounded bg-blue-600 text-xs leading-4 text-white hover:bg-blue-700"
            onClick={() => open(anchorOf(change, side))}
          >
            +
          </button>
        )}
        {renderDefault()}
      </>
    ),
    [viewType, open],
  );

  const widgets = useMemo(() => {
    const keys = new Set(placed.byChangeKey.keys());
    if (draftKey) keys.add(draftKey);
    const out: Record<string, ReactNode> = {};
    for (const key of keys) {
      const list = placed.byChangeKey.get(key);
      out[key] = (
        <div className="space-y-2 bg-gray-50 px-3 py-2 font-sans">
          {list && <CommentThread comments={list} onUpdate={onUpdate} onDelete={onDelete} drafts={drafts} />}
          {key === draftKey && <DraftInput drafts={drafts} onAdd={onAdd} path={path} view={view} />}
        </div>
      );
    }
    return out;
  }, [placed, draftKey, drafts, path, view, onAdd, onUpdate, onDelete]);

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
      <OutdatedComments {...commentProps} outdated={placed.outdated} draftOutdated={draftKey === null} />
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

type DraftProps = Pick<PatchViewProps, 'drafts' | 'onAdd' | 'path' | 'view'>;

/** 새 코멘트 입력. 본문은 `drafts`에 있어서 다시 mount되어도 남는다 */
function DraftInput({ drafts, onAdd, path, view }: DraftProps) {
  const { draft, change, close } = drafts;
  if (!draft) return null;
  const { anchor } = draft;
  return (
    <CommentInput
      body={draft.body}
      focusId="draft"
      focused={drafts.focused}
      onChange={change}
      onSave={async (body) => {
        await onAdd({ path, side: anchor.side, line: anchor.line, view, snippet: anchor.snippet, body });
        close();
      }}
      onCancel={close}
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
}: DraftProps & {
  outdated: Comment[];
  draftOutdated: boolean;
  onUpdate: OnUpdateComment;
  onDelete: OnDeleteComment;
}) {
  const draft = draftOutdated ? props.drafts.draft : null;
  const count = outdated.length + (draft ? 1 : 0);
  if (count === 0) return null;
  return (
    <details open className="border-b border-gray-200 bg-gray-50 px-3 py-2 text-sm">
      <summary className="cursor-pointer text-xs text-gray-600">위치가 바뀐 코멘트 {count}개</summary>
      <ul className="mt-2 space-y-3">
        {outdated.map((c) => (
          <li key={c.id}>
            <LineLabel {...c} />
            <SavedComment comment={c} onUpdate={props.onUpdate} onDelete={props.onDelete} drafts={props.drafts} />
          </li>
        ))}
        {draft && (
          <li>
            <LineLabel {...draft.anchor} />
            <DraftInput {...props} />
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
