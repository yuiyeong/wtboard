import { useCallback, useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { setViewed } from './api';
import type { ChangedFile } from './api';
import { filesKey } from './lib/sessions';
import { viewedErrorText, withViewed } from './lib/viewed';

interface Vars {
  file: ChangedFile;
  viewed: boolean;
  hash: string | null;
}

/**
 * 봤음 체크 mutation. 파일 목록을 먼저 바꾸고(optimistic) 실패하면 되돌리고 `error`로 알린다.
 * `error`는 다음 저장이 성공하면 지워진다. worktree가 바뀌면 WorktreeView가 새로 mount되어 지워진다.
 * `onSetViewed`는 참조가 안정적이라 memo된 컴포넌트에 넘겨도 된다.
 */
export function useSetViewed(worktreeId: string) {
  const queryClient = useQueryClient();
  const branchKey = filesKey(worktreeId, 'branch', null);
  const [error, setError] = useState<string | null>(null);
  const { mutate } = useMutation({
    mutationFn: ({ file, viewed, hash }: Vars) => setViewed(worktreeId, file.path, viewed, hash),
    onMutate: async ({ file, viewed }) => {
      await queryClient.cancelQueries({ queryKey: branchKey });
      const previous = queryClient.getQueryData<ChangedFile[]>(branchKey);
      if (previous) queryClient.setQueryData(branchKey, withViewed(previous, file.path, viewed));
      return { previous };
    },
    onError: (error, _vars, context) => {
      console.error('set_viewed 실패', error);
      if (context?.previous) queryClient.setQueryData(branchKey, context.previous);
      setError(viewedErrorText(error));
    },
    onSuccess: () => setError(null),
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: ['files', worktreeId] });
      queryClient.invalidateQueries({ queryKey: ['repos'] });
    },
  });
  const onSetViewed = useCallback(
    (file: ChangedFile, viewed: boolean, hash: string | null) => mutate({ file, viewed, hash }),
    [mutate],
  );
  return { onSetViewed, error };
}
