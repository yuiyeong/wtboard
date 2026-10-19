import { useState } from 'react';
import type { ChangedFile } from '../api';
import { scrollToFile } from '../lib/anchor';
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

const NO_UNSENT = new Map<string, number>();

/** `initiallyCollapsed`: 처음에 닫아 둘 디렉터리 path (접힌 파일 묶음). `unsent`: path별 미전송 코멘트 수 */
export function FileTree({
  nodes,
  initiallyCollapsed = [],
  unsent = NO_UNSENT,
}: {
  nodes: TreeNode[];
  initiallyCollapsed?: string[];
  unsent?: Map<string, number>;
}) {
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
      <Nodes nodes={nodes} depth={0} collapsed={collapsed} toggle={toggle} unsent={unsent} />
    </ul>
  );
}

interface NodesProps {
  nodes: TreeNode[];
  depth: number;
  collapsed: Set<string>;
  toggle: (path: string) => void;
  unsent: Map<string, number>;
}

function Nodes({ nodes, depth, collapsed, toggle, unsent }: NodesProps) {
  const pad = { paddingLeft: 12 + depth * INDENT_PX };
  return nodes.map((node) => {
    if (node.kind === 'file') {
      const f = node.file;
      const viewed = VIEWED_MARK[f.viewed];
      const comments = unsent.get(f.path) ?? 0;
      return (
        <li key={f.path}>
          <button
            className="flex w-full items-center gap-2 py-0.5 pr-3 text-left hover:bg-gray-100"
            style={pad}
            title={f.path}
            onClick={() => scrollToFile(f.path)}
          >
            <span className={`w-3 shrink-0 text-xs ${viewed.color}`}>{viewed.mark}</span>
            <span className={`w-3 font-mono text-xs ${STATUS_COLOR[f.status]}`}>{STATUS_LABEL[f.status]}</span>
            <span className={`truncate ${f.viewed === 'viewed' ? 'text-gray-400' : ''}`}>{node.name}</span>
            {comments > 0 && (
              <span className="shrink-0 text-xs" title={`미전송 코멘트 ${comments}개`}>
                💬
              </span>
            )}
            <span className="flex-1" />
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
            <Nodes nodes={node.children} depth={depth + 1} collapsed={collapsed} toggle={toggle} unsent={unsent} />
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

