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

const INDENT_PX = 14;

export function FileTree({ nodes }: { nodes: TreeNode[] }) {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
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
      return (
        <li key={f.path}>
          <button
            className="flex w-full items-center gap-2 py-0.5 pr-3 text-left hover:bg-gray-100"
            style={pad}
            title={f.path}
            onClick={() => document.getElementById(fileAnchorId(f.path))?.scrollIntoView({ block: 'start' })}
          >
            <span className={`w-3 font-mono text-xs ${STATUS_COLOR[f.status]}`}>{STATUS_LABEL[f.status]}</span>
            <span className="flex-1 truncate">{node.name}</span>
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

/** 생성·lock 파일 묶음. 기본으로 닫혀 있다 */
export function CollapsedGroup({ count, nodes }: { count: number; nodes: TreeNode[] }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="mt-2 border-t border-gray-200 pt-1">
      <button
        className="flex w-full items-center gap-1 px-3 py-0.5 text-left text-sm text-gray-500 hover:bg-gray-100"
        onClick={() => setOpen(!open)}
      >
        <span className="w-3 text-xs">{open ? '▾' : '▸'}</span>
        <span>접힌 파일 ({count})</span>
      </button>
      {open && <FileTree nodes={nodes} />}
    </div>
  );
}
