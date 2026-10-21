import { useEffect, useRef } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  addComment,
  clearSentComments,
  copyComments,
  deleteComment,
  listComments,
  markCommentsSent,
  resendComment,
  resolveComment,
  unresolveComment,
  updateComment,
  type NewComment,
} from './api';
import { commentsKey, copyUnsent } from './lib/comments';

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
 * 코멘트 mutation들 (추가·수정·삭제·해결됨·되돌리기·다시 보내기·전송됨 지우기·복사). 성공하면 코멘트 목록과 `['repos']`(미전송 수)를 다시 읽는다.
 * `copy`는 `copyUnsent`: 복사한 코멘트 수를 돌려준다 (미전송이 없으면 0)
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
  const resolve = useMutation({ mutationFn: (id: number) => resolveComment(id), onSuccess });
  const unresolve = useMutation({ mutationFn: (id: number) => unresolveComment(id), onSuccess });
  const resend = useMutation({ mutationFn: (id: number) => resendComment(id), onSuccess });
  const clearSent = useMutation({ mutationFn: () => clearSentComments(worktreeId), onSuccess });
  const copy = useMutation({
    mutationFn: () =>
      copyUnsent(
        () => copyComments(worktreeId),
        (markdown) => navigator.clipboard.writeText(markdown),
        markCommentsSent,
      ),
    onSuccess,
  });
  return { add, update, remove, resolve, unresolve, resend, clearSent, copy };
}
