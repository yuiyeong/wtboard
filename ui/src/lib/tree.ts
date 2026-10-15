import type { ChangedFile } from '../api';

export interface DirNode {
  kind: 'dir';
  /** 표시 이름. 합쳐진 경우 `services/content` 처럼 여러 단계 */
  name: string;
  /** worktree root 기준 전체 경로 (접기 상태 key) */
  path: string;
  children: TreeNode[];
}

export interface FileNode {
  kind: 'file';
  name: string;
  file: ChangedFile;
}

export type TreeNode = DirNode | FileNode;

/** 변경 파일 목록 → 탐색기 트리. 디렉터리가 먼저, 각각 이름순. 자식이 디렉터리 하나뿐이면 합친다. */
export function buildTree(files: ChangedFile[]): TreeNode[] {
  const root: DirNode = { kind: 'dir', name: '', path: '', children: [] };
  for (const file of files) {
    const parts = file.path.split('/');
    let dir = root;
    for (const part of parts.slice(0, -1)) {
      const path = dir.path ? `${dir.path}/${part}` : part;
      let next = dir.children.find((c): c is DirNode => c.kind === 'dir' && c.name === part);
      if (!next) {
        next = { kind: 'dir', name: part, path, children: [] };
        dir.children.push(next);
      }
      dir = next;
    }
    dir.children.push({ kind: 'file', name: parts[parts.length - 1], file });
  }
  return finish(root.children);
}

function finish(nodes: TreeNode[]): TreeNode[] {
  return nodes
    .map((n) => (n.kind === 'dir' ? compact({ ...n, children: finish(n.children) }) : n))
    .sort((a, b) => (a.kind === b.kind ? a.name.localeCompare(b.name) : a.kind === 'dir' ? -1 : 1));
}

function compact(dir: DirNode): DirNode {
  const [only] = dir.children;
  if (dir.children.length === 1 && only.kind === 'dir') {
    return { ...only, name: `${dir.name}/${only.name}` };
  }
  return dir;
}

/** 트리에 보이는 순서의 파일 목록. 오른쪽 diff 순서를 트리와 맞춘다. */
export function flattenFiles(nodes: TreeNode[]): ChangedFile[] {
  return nodes.flatMap((n) => (n.kind === 'file' ? [n.file] : flattenFiles(n.children)));
}
