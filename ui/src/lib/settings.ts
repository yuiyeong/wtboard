/** base 선택지. config에 적힌 base가 후보에 없으면(지워진 branch, tag, commit hash) 조용히 바꾸지 않고 맨 앞에 더한다 */
export function baseOptions(candidates: string[], current: string | null): string[] {
  return current === null || candidates.includes(current) ? candidates : [current, ...candidates];
}

/** `자동` 선택지 문구. 괄호 안은 `origin/HEAD`가 가리키는 branch */
export function autoLabel(autoBase: string | null): string {
  return `자동 (${autoBase ?? '정할 수 없음'})`;
}
