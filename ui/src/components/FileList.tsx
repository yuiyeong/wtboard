import type { ChangedFile } from '../api';
import { fileAnchorId } from '../lib/anchor';

const STATUS_LABEL: Record<ChangedFile['status'], string> = {
  added: 'A',
  modified: 'M',
  deleted: 'D',
  untracked: 'U',
};

export function FileList({ files }: { files: ChangedFile[] }) {
  return (
    <ul className="text-sm">
      {files.map((f) => (
        <li key={f.path}>
          <button
            className="flex w-full items-center gap-2 px-3 py-1 text-left hover:bg-gray-100"
            title={f.path}
            onClick={() => document.getElementById(fileAnchorId(f.path))?.scrollIntoView({ block: 'start' })}
          >
            <span className="w-4 font-mono text-gray-500">{STATUS_LABEL[f.status]}</span>
            <span className="flex-1 truncate">{f.path}</span>
            <LineCounts file={f} />
          </button>
        </li>
      ))}
    </ul>
  );
}

export function LineCounts({ file }: { file: ChangedFile }) {
  if (file.additions === null) return <span className="text-gray-400">bin</span>;
  return (
    <span className="tabular-nums">
      <span className="text-green-600">+{file.additions}</span>{' '}
      <span className="text-red-600">-{file.deletions}</span>
    </span>
  );
}
