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

// 여는 `"""`(2번 줄)가 hunk 밖에 있다
const OLD_SOURCE = 'def f():\n    """doc\n    more\n    """\n    return 1\n\ndef g():\n    """g"""\n';
const DOCSTRING_DIFF = [
  'diff --git a/a.py b/a.py',
  'index 1111111..2222222 100644',
  '--- a/a.py',
  '+++ b/a.py',
  '@@ -4,5 +4,5 @@ def f():',
  '     """',
  '-    return 1',
  '+    return 2',
  ' ',
  ' def g():',
  '     """g"""',
  '',
].join('\n');

type Node = { type: string; value?: string; properties?: { className?: string[] }; children?: Node[] };

/** 한 줄의 token 중 `value`를 감싼 class 이름들 */
function classesOf(line: Node[], value: string): string[] {
  const walk = (nodes: Node[], classes: string[]): string[] | undefined => {
    for (const n of nodes) {
      if (n.type === 'text' && n.value?.includes(value)) return classes;
      const found = n.children && walk(n.children, [...classes, ...(n.properties?.className ?? [])]);
      if (found) return found;
    }
  };
  return walk(line, []) ?? [];
}

describe('tokenizeHunks', () => {
  const [file] = parseDiff(DIFF);

  it('python 변경을 토큰화한다', () => {
    expect(tokenizeHunks(file.hunks, 'python')).toBeDefined();
  });

  it('모르는 언어는 하이라이팅을 건너뛴다', () => {
    expect(tokenizeHunks(file.hunks, null)).toBeUndefined();
    expect(tokenizeHunks(file.hunks, 'no-such-language')).toBeUndefined();
  });

  describe('hunk 밖의 여는 따옴표', () => {
    const [doc] = parseDiff(DOCSTRING_DIFF);
    const returnLine = (oldSource?: string | null) => tokenizeHunks(doc.hunks, 'python', oldSource)!.new[4] as Node[];

    it('old 파일 전체를 받으면 string 안팎을 바르게 본다', () => {
      expect(classesOf(returnLine(OLD_SOURCE), 'return')).toContain('keyword');
    });

    it('old 파일이 없으면 hunk만 본다 (닫는 따옴표를 여는 것으로 읽는다)', () => {
      expect(classesOf(returnLine(null), 'return')).not.toContain('keyword');
    });

    it('old 파일이 hunk와 맞지 않으면 쓰지 않는다', () => {
      // 엉뚱한 줄에 다른 파일의 token이 붙는 것보다 hunk만 보는 편이 낫다
      const other = 'x = 1\ny = 2\nz = 3\nw = 4\nreturn_value = 5\n\nv = 6\nu = 7\n';
      expect(classesOf(returnLine(other), 'return')).toEqual(classesOf(returnLine(null), 'return'));
    });
  });
});
