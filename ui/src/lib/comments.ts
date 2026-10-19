import { getChangeKey, type ChangeData } from 'react-diff-view';
import type { Comment, Side, View } from '../api';
import { formatHHMM } from './time';

export const commentsKey = (worktreeId: string) => ['comments', worktreeId] as const;

/** 서버가 snippet을 자르는 글자 수 (`MAX_SNIPPET_CHARS`). Rust `chars()`와 같게 UTF-16이 아니라 code point로 센다 */
const MAX_SNIPPET_CHARS = 500;

/** 줄 내용을 서버가 저장하는 snippet 형태로 자른다 */
function snippetOf(content: string): string {
  // 짧은 줄은 code point 배열을 만들지 않는다 (UTF-16 길이 >= code point 수)
  if (content.length <= MAX_SNIPPET_CHARS) return content;
  return Array.from(content).slice(0, MAX_SNIPPET_CHARS).join('');
}

export interface Anchor {
  side: Side;
  line: number;
  /** 그 줄의 내용 (500자까지). parseDiff의 `content`는 `+`/`-`/공백 접두가 없다 */
  snippet: string;
}

/**
 * 누른 change에서 코멘트 위치를 정한다. 삭제 줄은 old·옛 줄 번호, 추가 줄은 new,
 * 문맥 줄은 split 화면의 어느 쪽을 눌러도 new·새 줄 번호 (spec 5절).
 * `_clicked`는 누른 쪽인데 위치는 change 종류만으로 정해지므로 쓰지 않는다
 */
export function anchorOf(change: ChangeData, _clicked: Side): Anchor {
  if (change.type === 'delete') return { side: 'old', line: change.lineNumber, snippet: snippetOf(change.content) };
  if (change.type === 'insert') return { side: 'new', line: change.lineNumber, snippet: snippetOf(change.content) };
  return { side: 'new', line: change.newLineNumber, snippet: snippetOf(change.content) };
}

export interface PlacedComments {
  /** react-diff-view `getChangeKey` → 그 줄 아래에 보일 코멘트 (입력 순서 유지) */
  byChangeKey: Map<string, Comment[]>;
  /** 맞는 줄이 없는 코멘트. 파일 diff 맨 위에 snippet과 함께 보인다 */
  outdated: Comment[];
}

/**
 * 코멘트를 지금 diff의 줄에 맞춘다. (side, line)인 줄이 있고 내용이 snippet과 같아야 한다.
 * `old` side는 view마다 before가 다르므로 단 view와 지금 view가 같을 때만 맞춘다
 */
export function placeComments(comments: Comment[], changes: ChangeData[], view: View): PlacedComments {
  const bySideLine = new Map(changes.map((change) => [sideLineOf(change), change]));
  const byChangeKey = new Map<string, Comment[]>();
  const outdated: Comment[] = [];
  for (const c of comments) {
    const change = c.side === 'old' && c.view !== view ? undefined : bySideLine.get(`${c.side}:${c.line}`);
    if (!change || snippetOf(change.content) !== c.snippet) {
      outdated.push(c);
      continue;
    }
    const key = getChangeKey(change);
    const placed = byChangeKey.get(key);
    if (placed) placed.push(c);
    else byChangeKey.set(key, [c]);
  }
  return { byChangeKey, outdated };
}

/** 코멘트 위치 맞추기에 쓰는 `side:line` (anchorOf와 같은 규칙: 삭제 줄은 옛 번호, 나머지는 새 번호) */
function sideLineOf(change: ChangeData): string {
  if (change.type === 'delete') return `old:${change.lineNumber}`;
  if (change.type === 'insert') return `new:${change.lineNumber}`;
  return `new:${change.newLineNumber}`;
}

/**
 * 아직 저장하지 않은 새 코멘트 입력. diff가 실시간으로 바뀌거나 FileDiff가 다시 mount되어도 쓰던 글이
 * 남게 FileDiff 밖(`drafts.ts`)에 두고, 그릴 때마다 `placeDraft`로 줄을 다시 찾는다
 */
export interface Draft {
  anchor: Anchor;
  body: string;
  /** 입력을 연 view. 저장할 때 이 view로 저장하고, `old` side는 이 view에서만 줄에 맞춘다 */
  view: View;
}

/**
 * 입력을 둘 줄의 change key. 위치는 change key(줄 순서)가 아니라 (side, line)과 내용으로 맞춘다.
 * 맞는 줄이 없으면 null (위치가 바뀐 코멘트 묶음에 둔다). `old` side는 `placeComments`처럼 연 view에서만 맞춘다
 */
export function placeDraft(draft: Draft, changes: ChangeData[], view: View): string | null {
  const { anchor } = draft;
  if (anchor.side === 'old' && draft.view !== view) return null;
  const target = `${anchor.side}:${anchor.line}`;
  const change = changes.find((c) => sideLineOf(c) === target);
  return change && snippetOf(change.content) === anchor.snippet ? getChangeKey(change) : null;
}

/** 다른 줄의 `+`를 눌렀을 때. 쓰던 글이 있으면 버리지 않고 그대로 둔다 */
export function nextDraft(current: Draft | undefined, anchor: Anchor, view: View): Draft {
  if (current && current.body.trim() !== '') return current;
  return { anchor, body: '', view };
}

/** 목록 순서: 미전송 먼저 → path → line → id. 서버(`list_comments`)와 같다 */
export function sortForList(comments: Comment[]): Comment[] {
  return [...comments].sort(
    (a, b) =>
      Number(a.sentAt !== null) - Number(b.sentAt !== null) ||
      compare(a.path, b.path) ||
      a.line - b.line ||
      a.id - b.id,
  );
}

// SQLite의 ORDER BY path는 byte 순서라 localeCompare를 쓰지 않는다
const compare = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);

/** `미전송` / `전송됨 · 14:05` / `복사됨 · 14:05` (복사는 `sentTo`가 `copy`) */
export function statusLabel(c: Comment, fmtTime: (ms: number) => string = formatHHMM): string {
  if (c.sentAt === null) return '미전송';
  return `${c.sentTo === 'copy' ? '복사됨' : '전송됨'} · ${fmtTime(c.sentAt)}`;
}

/** path별 미전송 코멘트 수 (파일 header·목록의 💬 n) */
export function unsentByPath(comments: Comment[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const c of comments) if (c.sentAt === null) counts.set(c.path, (counts.get(c.path) ?? 0) + 1);
  return counts;
}

/** path별 코멘트 (입력 순서 유지). 파일 diff마다 자기 코멘트만 넘긴다 */
export function byPath(comments: Comment[]): Map<string, Comment[]> {
  const groups = new Map<string, Comment[]>();
  for (const c of comments) {
    const group = groups.get(c.path);
    if (group) group.push(c);
    else groups.set(c.path, [c]);
  }
  return groups;
}

/** 목록에 보일 body 한 줄: 내용이 있는 첫 줄 */
export function firstLine(body: string): string {
  return body.split('\n').find((line) => line.trim() !== '')?.trim() ?? '';
}
