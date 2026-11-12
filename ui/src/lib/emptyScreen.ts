/**
 * 열린 레포가 0개일 때 main 화면에 보일 안내.
 * `noRepos`: config에 등록이 없다. `unopened`: 등록은 있는데 하나도 열지 못했다 (폴더를 옮겼거나 지웠다).
 * `errors`는 경고(옛 hook binary 등)도 담으므로 판단에 쓰지 않는다
 */
export type EmptyScreen = 'noRepos' | 'unopened';

export function emptyScreen(configuredRepos: number, openedRepos: number): EmptyScreen | null {
  if (openedRepos > 0) return null;
  return configuredRepos === 0 ? 'noRepos' : 'unopened';
}
