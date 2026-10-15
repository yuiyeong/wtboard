#!/bin/sh
# UI 확인용 fixture 레포 3개(Python / Node / Flutter)와 config.toml을 만든다. 실행할 때마다 새로 만든다.
# 사용: scripts/fixtures.sh <dir>
#       WTBOARD_CONFIG=<dir>/config.toml pnpm tauri dev
set -eu
[ $# -eq 1 ] || { echo "usage: $0 <dir>" >&2; exit 1; }
mkdir -p "$1"
DIR=$(cd "$1" && pwd)
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
g() { git -c user.email=fixture@example.com -c user.name=fixture -c commit.gpgsign=false "$@"; }

# $1 worktree, $2 경로, $3 내용
put() { mkdir -p "$(dirname "$1/$2")"; printf '%b' "$3" > "$1/$2"; }

# $1 이름. stdin의 "경로<TAB>내용" 줄로 upstream main을 만들고 clone + feat worktree를 연다
repo() {
  name=$1
  rm -rf "$DIR/upstream-$name" "$DIR/$name" "$DIR/$name-feat"
  mkdir -p "$DIR/upstream-$name"
  while IFS="	" read -r path body; do
    put "$DIR/upstream-$name" "$path" "$body"
  done
  g -C "$DIR/upstream-$name" init -q -b main
  g -C "$DIR/upstream-$name" add -A
  g -C "$DIR/upstream-$name" commit -q -m init
  g clone -q "$DIR/upstream-$name" "$DIR/$name"
  g -C "$DIR/$name" worktree add -q -b feat "$DIR/$name-feat"
}


repo py <<'EOF'
requirements.txt	fastapi\n
.gitignore	venv/\n
app/main.py	from fastapi import FastAPI\n\napp = FastAPI()\n
EOF
put "$DIR/py-feat" app/main.py 'from fastapi import FastAPI\n\napp = FastAPI(title="fixture")\n\n\n@app.get("/health")\ndef health():\n    return {"ok": True}\n'
put "$DIR/py-feat" app/users.py 'def list_users():\n    return []\n'
put "$DIR/py-feat" uv.lock 'version = 1\n'
put "$DIR/py-feat" venv/lib/site.py '# 무시됨\n'

repo node <<'EOF'
package.json	{ "dependencies": { "next": "15.0.0" } }\n
.gitattributes	generated/** linguist-generated\n
app/page.tsx	export default function Page() {\n  return <h1>hello</h1>;\n}\n
EOF
put "$DIR/node-feat" app/page.tsx 'export default function Page() {\n  return <h1>hello, fixture</h1>;\n}\n'
put "$DIR/node-feat" package-lock.json '{ "lockfileVersion": 3 }\n'
put "$DIR/node-feat" next-env.d.ts '/// <reference types="next" />\n'
put "$DIR/node-feat" generated/api.ts 'export const api = {};\n'

repo flutter <<'EOF'
pubspec.yaml	name: fixture\n
.gitignore	build/\n
lib/user.dart	class User {}\n
EOF
put "$DIR/flutter-feat" lib/user.dart 'class User {\n  final String name;\n  User(this.name);\n}\n'
put "$DIR/flutter-feat" lib/user.g.dart '// GENERATED CODE\n'
put "$DIR/flutter-feat" pubspec.lock 'packages: {}\n'
put "$DIR/flutter-feat" build/app.txt '무시됨\n'

cat > "$DIR/config.toml" <<EOF
[[repo]]
path = "$DIR/py"

[[repo]]
path = "$DIR/node-feat"

[[repo]]
path = "$DIR/flutter"

[[repo]]
path = "$DIR/없는-레포"
EOF
echo "fixtures: $DIR"
echo "run: WTBOARD_CONFIG=$DIR/config.toml pnpm tauri dev"
