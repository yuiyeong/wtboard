export const fileAnchorId = (path: string) => `file-${encodeURIComponent(path)}`;

/** 파일 diff로 스크롤한다. 지금 목록에 없는 파일이면 아무것도 하지 않는다 */
export function scrollToFile(path: string) {
  document.getElementById(fileAnchorId(path))?.scrollIntoView({ block: 'start' });
}
