import { describe, expect, it } from 'vitest';
import { languageForPath } from './language';

describe('languageForPath', () => {
  it.each([
    ['app/api/users.py', 'python'],
    ['src/App.tsx', 'tsx'],
    ['lib/user.g.dart', 'dart'],
    ['pnpm-lock.yaml', 'yaml'],
    ['README.MD', 'markdown'],
    ['Dockerfile', null],
    ['.env', null],
  ])('%s → %s', (path, lang) => {
    expect(languageForPath(path)).toBe(lang);
  });
});
