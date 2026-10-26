import refractor from 'refractor';
import { computeOldLineNumber, tokenize, type HunkData } from 'react-diff-view';

/**
 * 하이라이팅 토큰. 모르는 언어거나 실패하면 undefined (하이라이팅 없이 렌더).
 * `oldSource`(old 쪽 파일 전체)가 있으면 hunk 밖의 줄까지 보고 하이라이팅한다.
 * 없으면 hunk 밖을 빈 줄로 두므로, hunk 밖에서 열린 string·comment의 안팎이 뒤집힐 수 있다
 */
export function tokenizeHunks(hunks: HunkData[], language: string | null, oldSource?: string | null) {
  if (!language || !refractor.registered(language)) return undefined;
  const source = oldSource && matchesHunks(oldSource, hunks) ? oldSource : undefined;
  try {
    return tokenize(hunks, { highlight: true, refractor, language, oldSource: source });
  } catch (e) {
    console.warn(`[wtboard] ${language} 하이라이팅 실패`, e);
    return undefined;
  }
}

/** old 쪽 줄(normal·delete)이 모두 `oldSource`의 같은 줄과 같아야 diff를 다시 적용할 수 있다 */
function matchesHunks(oldSource: string, hunks: HunkData[]): boolean {
  const lines = oldSource.split('\n');
  return hunks.every((hunk) =>
    hunk.changes.every((c) => {
      const n = computeOldLineNumber(c); // insert는 -1
      return n < 0 || lines[n - 1] === c.content;
    }),
  );
}
