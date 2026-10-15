import { memo, useMemo, useState } from 'react';
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

// 레포 목록이 갱신될 때마다(파일 event마다) 화면 전체가 다시 그려지므로, 바뀐 파일만 다시 그린다
export const FileDiff = memo(function FileDiff({ worktreeId, file, viewType }: Props) {
  const [ref, inView] = useInView<HTMLElement>();
  // 생성 파일은 접힌 채로 시작한다. 접혀 있으면 patch를 계산하지 않는다
  const [open, setOpen] = useState(!file.collapsed);
  const patch = useQuery({
    queryKey: ['patch', worktreeId, file.path, 'branch'],
    queryFn: () => getPatch(worktreeId, 'branch', file.path),
    enabled: inView && open, // 화면에 보이고 펼쳐져 있을 때만 계산
  });

  return (
    <section ref={ref} id={fileAnchorId(file.path)} className="mb-4 rounded border border-gray-200">
      <header
        className="sticky top-0 z-10 flex cursor-pointer items-center gap-3 border-b border-gray-200 bg-gray-50 px-3 py-2 text-sm"
        onClick={() => setOpen(!open)}
      >
        <span className="w-3 text-xs text-gray-500">{open ? '▾' : '▸'}</span>
        <span className="flex-1 truncate font-mono">{file.path}</span>
        {file.collapsed && !open && <span className="text-xs text-gray-400">생성 파일</span>}
        <LineCounts file={file} />
      </header>
      {open &&
        (patch.error ? (
          <p className="p-3 text-sm text-red-600">{String(patch.error)}</p>
        ) : patch.data ? (
          <PatchView body={patch.data.body} viewType={viewType} path={file.path} />
        ) : (
          <div className="h-20" />
        ))}
    </section>
  );
});

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
