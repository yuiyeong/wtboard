import { useEffect, useRef, useState } from 'react';

/** 요소가 화면(위아래 rootMargin 포함)에 들어와 있는지. patch lazy loading에 쓴다. */
export function useInView<T extends Element>(rootMargin = '400px') {
  const ref = useRef<T>(null);
  const [inView, setInView] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const observer = new IntersectionObserver(([entry]) => setInView(entry.isIntersecting), { rootMargin });
    observer.observe(el);
    return () => observer.disconnect();
  }, [rootMargin]);
  return [ref, inView] as const;
}
