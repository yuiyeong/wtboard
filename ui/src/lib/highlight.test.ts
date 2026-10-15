import { describe, expect, it } from 'vitest';
import { parseDiff } from 'react-diff-view';
import { tokenizeHunks } from './highlight';

const DIFF = [
  'diff --git a/a.py b/a.py',
  'index 1111111..2222222 100644',
  '--- a/a.py',
  '+++ b/a.py',
  '@@ -1 +1 @@',
  '-x = 1',
  '+x = 2',
  '',
].join('\n');

describe('tokenizeHunks', () => {
  const [file] = parseDiff(DIFF);

  it('python 변경을 토큰화한다', () => {
    expect(tokenizeHunks(file.hunks, 'python')).toBeDefined();
  });

  it('모르는 언어는 하이라이팅을 건너뛴다', () => {
    expect(tokenizeHunks(file.hunks, null)).toBeUndefined();
    expect(tokenizeHunks(file.hunks, 'no-such-language')).toBeUndefined();
  });
});
