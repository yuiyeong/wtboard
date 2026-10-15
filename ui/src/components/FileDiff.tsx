import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Diff, Hunk, parseDiff } from 'react-diff-view';
import { getPatch, type ChangedFile, type PatchBody } from '../api';
import { fileAnchorId } from '../lib/anchor';
import { tokenizeHunks } from '../lib/highlight';
import { languageForPath } from '../lib/language';
import { useInView } from '../lib/useInView';
import type { ViewType } from '../store';
import { LineCounts } from './FileTree';

interface Props {
  worktreeId: string;
  file: ChangedFile;
  viewType: ViewType;
}

export function FileDiff({ worktreeId, file, viewType }: Props) {
  const [ref, inView] = useInView<HTMLElement>();
  const patch = useQuery({
    queryKey: ['patch', worktreeId, file.path, 'branch'],
    queryFn: () => getPatch(worktreeId, 'branch', file.path),
    enabled: inView, // 화면에 보일 때만 계산
  });

  return (
    <section ref={ref} id={fileAnchorId(file.path)} className="mb-4 rounded border border-gray-200">
      <header className="sticky top-0 z-10 flex items-center gap-3 border-b border-gray-200 bg-gray-50 px-3 py-2 text-sm">
        <span className="flex-1 truncate font-mono">{file.path}</span>
        <LineCounts file={file} />
      </header>
      {patch.error ? (
        <p className="p-3 text-sm text-red-600">{String(patch.error)}</p>
      ) : patch.data ? (
        <PatchView body={patch.data.body} viewType={viewType} path={file.path} />
      ) : (
        <div className="h-20" />
      )}
    </section>
  );
}

function PatchView({ body, viewType, path }: { body: PatchBody; viewType: ViewType; path: string }) {
  if (body.kind === 'text') return <TextDiff diff={body.diff} viewType={viewType} path={path} />;
  const label = body.kind === 'binary' ? '바이너리 파일' : '1MB 초과 파일';
  return (
    <p className="p-3 text-sm text-gray-500">
      {label} 변경됨 ({formatSize(body.oldSize)} → {formatSize(body.newSize)})
    </p>
  );
}

function TextDiff({ diff, viewType, path }: { diff: string; viewType: ViewType; path: string }) {
  const file = useMemo(() => parseDiff(diff)[0], [diff]);
  const tokens = useMemo(() => (file ? tokenizeHunks(file.hunks, languageForPath(path)) : undefined), [file, path]);
  if (!file || file.hunks.length === 0) {
    return <p className="p-3 text-sm text-gray-500">내용 변경 없음 (빈 파일 또는 권한 변경)</p>;
  }
  return (
    <Diff viewType={viewType} diffType={file.type} hunks={file.hunks} tokens={tokens}>
      {(hunks) => hunks.map((hunk) => <Hunk key={hunk.content} hunk={hunk} />)}
    </Diff>
  );
}

function formatSize(n: number | null) {
  if (n === null) return '없음';
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
