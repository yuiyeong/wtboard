import { describe, expect, it } from 'vitest';
import { MAX_ZOOM, MIN_ZOOM, zoomForKey } from './zoom';

const cmd = (key: string) => ({ key, metaKey: true });

describe('zoomForKey', () => {
  it('Cmd = / Cmd + 는 10% 키운다', () => {
    expect(zoomForKey(cmd('='), 1)).toBe(1.1);
    expect(zoomForKey(cmd('+'), 1.1)).toBe(1.2);
  });

  it('Cmd - 는 10% 줄이고, Cmd 0은 원래 크기로 돌린다', () => {
    expect(zoomForKey(cmd('-'), 1)).toBe(0.9);
    expect(zoomForKey(cmd('0'), 1.7)).toBe(1);
  });

  it('여러 번 눌러도 소수 오차가 쌓이지 않는다', () => {
    let z = 1;
    for (let i = 0; i < 3; i++) z = zoomForKey(cmd('='), z)!;
    expect(z).toBe(1.3);
  });

  it('범위 밖으로 나가지 않는다', () => {
    expect(zoomForKey(cmd('-'), MIN_ZOOM)).toBe(MIN_ZOOM);
    expect(zoomForKey(cmd('='), MAX_ZOOM)).toBe(MAX_ZOOM);
  });

  it('Cmd 없이 누르거나 다른 키면 null', () => {
    expect(zoomForKey({ key: '=', metaKey: false }, 1)).toBeNull();
    expect(zoomForKey(cmd('a'), 1)).toBeNull();
  });
});
