#!/bin/sh
# Claude Code hook 진입점. wtboard-hook이 설치돼 있지 않으면 아무것도 하지 않는다.
# WTBOARD_HOOK_BIN으로 binary 경로를 바꿀 수 있다 (test·fixture용).
BIN="${WTBOARD_HOOK_BIN:-$HOME/.wtboard/bin/wtboard-hook}"
[ -x "$BIN" ] || exit 0
exec "$BIN" "$@"
