import { describe, expect, it } from 'vitest';
import { autoLabel, baseOptions } from './settings';

describe('baseOptions', () => {
  const candidates = ['origin/main', 'main'];

  it('config의 base가 후보에 없으면 맨 앞에 더한다 (지워진 branch, tag 등)', () => {
    expect(baseOptions(candidates, 'v1.2')).toEqual(['v1.2', 'origin/main', 'main']);
  });

  it('후보에 있거나 자동(null)이면 후보 그대로', () => {
    expect(baseOptions(candidates, 'origin/main')).toEqual(candidates);
    expect(baseOptions(candidates, null)).toEqual(candidates);
  });
});

describe('autoLabel', () => {
  it('origin/HEAD가 가리키는 branch를 괄호로 적는다', () => {
    expect(autoLabel('origin/develop')).toBe('자동 (origin/develop)');
  });

  it('origin/HEAD가 없으면 정할 수 없음', () => {
    expect(autoLabel(null)).toBe('자동 (정할 수 없음)');
  });
});
