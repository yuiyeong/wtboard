import { useEffect, useState } from 'react';

/** 상대 시각 표시를 위해 주기적으로 바뀌는 현재 시각 */
export function useNow(intervalMs: number) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}
