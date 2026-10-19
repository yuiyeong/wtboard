import { getChangeKey, type ChangeData } from 'react-diff-view';
import type { Comment, Side, View } from '../api';

export const commentsKey = (worktreeId: string) => ['comments', worktreeId] as const;

export interface Anchor {
  side: Side;
  line: number;
  /** 그 줄의 내용. parseDiff의 `content`는 `+`/`-`/공백 접두가 없다 */
  snippet: string;
}

/**
 * 누른 change에서 코멘트 위치를 정한다. 삭제 줄은 old·옛 줄 번호, 추가 줄은 new,
 * 문맥 줄은 split 화면의 어느 쪽을 눌러도 new·새 줄 번호 (spec 5절).
 * `_clicked`는 누른 쪽인데 위치는 change 종류만으로 정해지므로 쓰지 않는다
 */
export function anchorOf(change: ChangeData, _clicked: Side): Anchor {
  if (change.type === 'delete') return { side: 'old', line: change.lineNumber, snippet: change.content };
  if (change.type === 'insert') return { side: 'new', line: change.lineNumber, snippet: change.content };
  return { side: 'new', line: change.newLineNumber, snippet: change.content };
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
  const bySideLine = new Map<string, ChangeData>();
  for (const change of changes) {
    if (change.type === 'delete') bySideLine.set(`old:${change.lineNumber}`, change);
    else if (change.type === 'insert') bySideLine.set(`new:${change.lineNumber}`, change);
    else bySideLine.set(`new:${change.newLineNumber}`, change);
  }
  const byChangeKey = new Map<string, Comment[]>();
  const outdated: Comment[] = [];
  for (const c of comments) {
    const change = c.side === 'old' && c.view !== view ? undefined : bySideLine.get(`${c.side}:${c.line}`);
    if (!change || change.content !== c.snippet) {
      outdated.push(c);
      continue;
    }
    const key = getChangeKey(change);
    byChangeKey.set(key, [...(byChangeKey.get(key) ?? []), c]);
  }
  return { byChangeKey, outdated };
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

/** 로컬 시각 `HH:MM` (24시간) */
export function formatHHMM(ms: number): string {
  const d = new Date(ms);
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
}

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
