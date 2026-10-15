import refractor from 'refractor';
import { tokenize, type HunkData } from 'react-diff-view';

/** 하이라이팅 토큰. 모르는 언어거나 실패하면 undefined (하이라이팅 없이 렌더) */
export function tokenizeHunks(hunks: HunkData[], language: string | null) {
  if (!language || !refractor.registered(language)) return undefined;
  try {
    return tokenize(hunks, { highlight: true, refractor, language });
  } catch (e) {
    console.warn(`[wtboard] ${language} 하이라이팅 실패`, e);
    return undefined;
  }
}
