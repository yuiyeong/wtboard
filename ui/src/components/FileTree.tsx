import { useState } from 'react';
import type { ChangedFile } from '../api';
import { fileAnchorId } from '../lib/anchor';
import type { TreeNode } from '../lib/tree';

const STATUS_LABEL: Record<ChangedFile['status'], string> = {
  added: 'A',
  modified: 'M',
  deleted: 'D',
  untracked: 'U',
};

const STATUS_COLOR: Record<ChangedFile['status'], string> = {
  added: 'text-green-600',
  modified: 'text-amber-600',
  deleted: 'text-red-600',
  untracked: 'text-green-600',
};

/** status 글자 앞의 봤음 표시. 미확인은 빈칸 */
const VIEWED_MARK: Record<ChangedFile['viewed'], { mark: string; color: string }> = {
  none: { mark: '', color: '' },
  viewed: { mark: '☑', color: 'text-gray-400' },
  changed: { mark: '●', color: 'text-amber-600' },
};

const INDENT_PX = 14;

/** `initiallyCollapsed`: 처음에 닫아 둘 디렉터리 path (접힌 파일 묶음) */
export function FileTree({ nodes, initiallyCollapsed = [] }: { nodes: TreeNode[]; initiallyCollapsed?: string[] }) {
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set(initiallyCollapsed));
  const toggle = (path: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  return (
    <ul className="text-sm">
      <Nodes nodes={nodes} depth={0} collapsed={collapsed} toggle={toggle} />
    </ul>
  );
}

interface NodesProps {
  nodes: TreeNode[];
  depth: number;
  collapsed: Set<string>;
  toggle: (path: string) => void;
}

function Nodes({ nodes, depth, collapsed, toggle }: NodesProps) {
  const pad = { paddingLeft: 12 + depth * INDENT_PX };
  return nodes.map((node) => {
    if (node.kind === 'file') {
      const f = node.file;
      const viewed = VIEWED_MARK[f.viewed];
      return (
        <li key={f.path}>
          <button
            className="flex w-full items-center gap-2 py-0.5 pr-3 text-left hover:bg-gray-100"
            style={pad}
            title={f.path}
            onClick={() => document.getElementById(fileAnchorId(f.path))?.scrollIntoView({ block: 'start' })}
          >
            <span className={`w-3 shrink-0 text-xs ${viewed.color}`}>{viewed.mark}</span>
            <span className={`w-3 font-mono text-xs ${STATUS_COLOR[f.status]}`}>{STATUS_LABEL[f.status]}</span>
            <span className={`flex-1 truncate ${f.viewed === 'viewed' ? 'text-gray-400' : ''}`}>{node.name}</span>
            <LineCounts file={f} />
          </button>
        </li>
      );
    }
    const isCollapsed = collapsed.has(node.path);
    return (
      <li key={node.path}>
        <button
          className="flex w-full items-center gap-1 py-0.5 pr-3 text-left text-gray-700 hover:bg-gray-100"
          style={pad}
          title={node.path}
          onClick={() => toggle(node.path)}
        >
          <span className="w-3 text-xs text-gray-400">{isCollapsed ? '▸' : '▾'}</span>
          <span className="truncate font-medium">{node.name}</span>
        </button>
        {!isCollapsed && (
          <ul>
            <Nodes nodes={node.children} depth={depth + 1} collapsed={collapsed} toggle={toggle} />
          </ul>
        )}
      </li>
    );
  });
}

export function LineCounts({ file }: { file: ChangedFile }) {
  if (file.additions === null) return <span className="text-gray-400">bin</span>;
  return (
    <span className="shrink-0 tabular-nums text-xs">
      <span className="text-green-600">+{file.additions}</span>{' '}
      <span className="text-red-600">-{file.deletions}</span>
    </span>
  );
}

