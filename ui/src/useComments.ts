import { useEffect, useRef } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  addComment,
  clearSentComments,
  copyComments,
  deleteComment,
  listComments,
  markCommentsSent,
  updateComment,
  type NewComment,
} from './api';
import { commentsKey } from './lib/comments';

/** hook이 prompt 때 전송됨으로 바꾸는 것을 따라가려고 미전송이 있는 동안만 주기적으로 다시 읽는다 */
const UNSENT_POLL_MS = 5000;

/**
 * worktree의 코멘트 목록. 미전송 집합이 바뀌면(hook이 전송했거나 지웠을 때) worktree별 미전송 수를
 * 담은 `['repos']`도 다시 읽는다
 */
export function useComments(worktreeId: string) {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: commentsKey(worktreeId),
    queryFn: () => listComments(worktreeId),
    refetchInterval: (q) => (q.state.data?.some((c) => c.sentAt === null) ? UNSENT_POLL_MS : false),
  });
  const unsentIds = query.data?.filter((c) => c.sentAt === null).map((c) => c.id).join(',');
  const previous = useRef(unsentIds);
  useEffect(() => {
    if (previous.current !== undefined && unsentIds !== previous.current) {
      queryClient.invalidateQueries({ queryKey: ['repos'] });
    }
    previous.current = unsentIds;
  }, [unsentIds, queryClient]);
  return query;
}

/**
 * 코멘트 mutation들. 성공하면 코멘트 목록과 `['repos']`(미전송 수)를 다시 읽는다.
 * `copy`는 markdown을 clipboard에 쓴 뒤에만 전송됨(복사)으로 표시하고, 복사한 코멘트 수를 돌려준다
 */
export function useCommentActions(worktreeId: string) {
  const queryClient = useQueryClient();
  const onSuccess = () => {
    queryClient.invalidateQueries({ queryKey: commentsKey(worktreeId) });
    queryClient.invalidateQueries({ queryKey: ['repos'] });
  };
  const add = useMutation({ mutationFn: (c: NewComment) => addComment(worktreeId, c), onSuccess });
  const update = useMutation({
    mutationFn: ({ id, body }: { id: number; body: string }) => updateComment(id, body),
    onSuccess,
  });
  const remove = useMutation({ mutationFn: (id: number) => deleteComment(id), onSuccess });
  const clearSent = useMutation({ mutationFn: () => clearSentComments(worktreeId), onSuccess });
  const copy = useMutation({
    mutationFn: async () => {
      const { markdown, ids } = await copyComments(worktreeId);
      await navigator.clipboard.writeText(markdown);
      await markCommentsSent(ids);
      return ids.length;
    },
    onSuccess,
  });
  return { add, update, remove, clearSent, copy };
}
