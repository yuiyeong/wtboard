import { describe, expect, it } from 'vitest';
import type { ChangedFile } from '../api';
import { buildTree, flattenFiles, type TreeNode } from './tree';

const f = (path: string): ChangedFile => ({ path, status: 'modified', additions: 1, deletions: 0 });

/** 테스트용 요약: 디렉터리는 'name/', 파일은 이름 */
function shape(nodes: TreeNode[]): unknown[] {
  return nodes.map((n) => (n.kind === 'dir' ? { [`${n.name}/`]: shape(n.children) } : n.name));
}

describe('buildTree', () => {
  it('디렉터리별로 묶고, 디렉터리를 파일보다 먼저 이름순으로 둔다', () => {
    const tree = buildTree([f('z.py'), f('app/b.py'), f('app/a.py'), f('tests/t.py'), f('a.py')]);
    expect(shape(tree)).toEqual([{ 'app/': ['a.py', 'b.py'] }, { 'tests/': ['t.py'] }, 'a.py', 'z.py']);
  });

  it('자식이 디렉터리 하나뿐이면 한 줄로 합친다', () => {
    const tree = buildTree([f('app/services/content/a.py'), f('app/services/content/b.py'), f('app/main.py')]);
    expect(shape(tree)).toEqual([{ 'app/': [{ 'services/content/': ['a.py', 'b.py'] }, 'main.py'] }]);
    const services = (tree[0] as Extract<TreeNode, { kind: 'dir' }>).children[0];
    expect(services.kind === 'dir' && services.path).toBe('app/services/content');
  });
});

describe('flattenFiles', () => {
  it('트리에 보이는 순서대로 파일을 돌려준다', () => {
    const files = [f('z.py'), f('app/b.py'), f('app/a.py')];
    expect(flattenFiles(buildTree(files)).map((x) => x.path)).toEqual(['app/a.py', 'app/b.py', 'z.py']);
  });
});
