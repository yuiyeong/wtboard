import { useCallback } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { setViewed } from './api';
import type { ChangedFile } from './api';
import { withViewed } from './lib/viewed';

interface Vars {
  file: ChangedFile;
  viewed: boolean;
  hash: string | null;
}

/**
 * 봤음 체크 mutation. 파일 목록을 먼저 바꾸고(optimistic) 실패하면 되돌린다.
 * 반환 함수는 참조가 안정적이라 memo된 컴포넌트에 넘겨도 된다.
 */
export function useSetViewed(worktreeId: string) {
  const queryClient = useQueryClient();
  const filesKey = ['files', worktreeId, 'branch'];
  const { mutate } = useMutation({
    mutationFn: ({ file, viewed, hash }: Vars) => setViewed(worktreeId, file.path, viewed, hash),
    onMutate: async ({ file, viewed }) => {
      await queryClient.cancelQueries({ queryKey: filesKey });
      const previous = queryClient.getQueryData<ChangedFile[]>(filesKey);
      if (previous) queryClient.setQueryData(filesKey, withViewed(previous, file.path, viewed));
      return { previous };
    },
    onError: (error, _vars, context) => {
      console.error('set_viewed 실패', error);
      if (context?.previous) queryClient.setQueryData(filesKey, context.previous);
    },
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: ['files', worktreeId] });
      queryClient.invalidateQueries({ queryKey: ['repos'] });
    },
  });
  return useCallback(
    (file: ChangedFile, viewed: boolean, hash: string | null) => mutate({ file, viewed, hash }),
    [mutate],
  );
}
