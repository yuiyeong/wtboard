export const MIN_ZOOM = 0.5;
export const MAX_ZOOM = 3;
const STEP = 0.1;

/** VS Code처럼 Cmd = (+) / Cmd - / Cmd 0. zoom 키가 아니면 null */
export function zoomForKey(e: { key: string; metaKey: boolean }, current: number): number | null {
  if (!e.metaKey) return null;
  let next: number;
  if (e.key === '=' || e.key === '+') next = current + STEP;
  else if (e.key === '-') next = current - STEP;
  else if (e.key === '0') next = 1;
  else return null;
  // 0.1을 계속 더하면 1.2000000000000002 같은 오차가 쌓인다
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Math.round(next * 10) / 10));
}
