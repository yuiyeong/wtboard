#!/bin/sh
# wtboard-hook을 release로 빌드해 ~/.wtboard/bin/에 설치하고, plugin 설치 명령을 안내한다.
# Claude 설정은 건드리지 않는다. 안내된 명령은 직접 실행한다.
# 사용: scripts/install-hook.sh
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
DEST="$HOME/.wtboard/bin"

[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
cd "$ROOT"
cargo build --release -p wtboard-hook

mkdir -p "$DEST"
# 실행 중인 binary를 제자리에서 덮어쓰지 않도록 같은 폴더에 복사한 뒤 rename한다
cp "$ROOT/target/release/wtboard-hook" "$DEST/wtboard-hook.tmp"
mv -f "$DEST/wtboard-hook.tmp" "$DEST/wtboard-hook"
echo "설치했습니다: $DEST/wtboard-hook"
echo
echo "Claude Code plugin은 아래 두 명령으로 직접 설치하세요."
echo "  claude plugin marketplace add \"$ROOT/plugin\""
echo "  claude plugin install wtboard@wtboard-local"
