const BY_EXTENSION: Record<string, string> = {
  py: 'python',
  ts: 'typescript',
  tsx: 'tsx',
  js: 'javascript',
  jsx: 'jsx',
  mjs: 'javascript',
  dart: 'dart',
  json: 'json',
  toml: 'toml',
  yaml: 'yaml',
  yml: 'yaml',
  md: 'markdown',
  css: 'css',
  html: 'markup',
  rs: 'rust',
  sh: 'bash',
  sql: 'sql',
};

/** 파일 경로 → refractor 언어 이름. 모르면 null */
export function languageForPath(path: string): string | null {
  const name = path.split('/').pop() ?? '';
  const dot = name.lastIndexOf('.');
  if (dot <= 0) return null;
  return BY_EXTENSION[name.slice(dot + 1).toLowerCase()] ?? null;
}
