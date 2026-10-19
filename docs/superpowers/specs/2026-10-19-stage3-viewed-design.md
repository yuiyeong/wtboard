# 3단계 설계: 봤음 체크 + 다시 바뀜 표시

상위 설계: `docs/draft.md` (10절, 12절, 13절). 이 문서는 3단계에서 draft와 달라진 점과 구체 설계를 적는다.

사용자는 질문 없이 바로 진행하기로 했다. 아래에서 "(내 판단)"으로 표시한 것은 대화에서 정하지 않고 설계자가 정한 것이다.

## 목표와 완료 기준

- 파일마다 "봤음"을 체크할 수 있다. 체크 시점의 diff hash를 저장하고, 앱을 다시 켜도 남는다.
- 체크한 파일의 diff가 바뀌면 (Claude가 고치면) 1초 안에 체크가 풀리고 "다시 바뀜"으로 보인다.
- header에 `확인 n/m`, sidebar의 worktree 줄에 미확인 수가 보인다.

범위 밖:
- "다시 바뀜" 파일에서 지난번 본 뒤로 바뀐 부분만 보여 주기 (interdiff). 전체 diff를 다시 보여 준다 (내 판단).
- 단축키, 일괄 체크.
- 2단계에서 넘어온 fork 레포 `upstream` PR base, worktree summary cache. `progress.md`에 그대로 둔다.

## draft와 달라진 점

| draft | 3단계 | 이유 |
|---|---|---|
| `set_viewed(worktree_id, path, diff_hash)` | `set_viewed(worktree_id, path, viewed, hash?)` | 체크 해제가 필요하다. 접힌 생성 파일처럼 patch를 아직 안 가져온 파일은 hash가 없으므로 core가 계산한다 |
| 봤음 상태는 worktree별 | worktree + 브랜치별 (내 판단) | 같은 worktree에서 브랜치를 오가도 각 브랜치의 체크가 남는다 |
| `state.sqlite`는 `~/.wtboard/` | config 파일과 같은 폴더 (내 판단) | `WTBOARD_CONFIG`로 fixture를 띄울 때 실제 상태를 건드리지 않는다. 기본 경로는 draft와 같다 |

## 1. 저장

`core::store::viewed` (신규). `rusqlite`(`bundled` feature)로 `<config 폴더>/state.sqlite`를 연다.

```sql
CREATE TABLE IF NOT EXISTS viewed (
  worktree  TEXT NOT NULL,     -- worktree id (root 경로)
  branch    TEXT NOT NULL,     -- detached HEAD면 ''
  path      TEXT NOT NULL,
  hash      TEXT NOT NULL,     -- 체크할 때의 diff hash
  viewed_at INTEGER NOT NULL,  -- Unix ms
  PRIMARY KEY (worktree, branch, path)
);
```

- `ViewedStore`는 `Mutex<Connection>` 하나를 들고 `Arc`로 모든 `Worktree`가 나눠 쓴다. `RepoOptions`로 넘긴다.
- API: `load(worktree, branch) -> HashMap<path, hash>`, `set(worktree, branch, path, hash)`, `remove(worktree, branch, path)`.
- DB를 열 수 없으면 in-memory DB로 계속하고 `Registry` 오류 목록에 "봤음 상태를 저장할 수 없음: …"을 남긴다 (내 판단). 앱은 계속 쓸 수 있고, 꺼지면 체크가 사라진다.
- 목록에서 빠진 파일(revert 등)의 row는 지우지 않는다. 같은 diff로 돌아오면 다시 봤음으로 보인다. 사라진 worktree의 row도 그대로 둔다 (정리는 범위 밖).
- 레포에는 아무것도 쓰지 않는다. DB는 레포 밖에 있다.

## 2. core: `Worktree`

봤음 상태는 저장된 hash와 현재 hash를 비교해 계산한다.

| 상태 | 조건 |
|---|---|
| `none` | row 없음 |
| `viewed` | row hash == 현재 hash |
| `changed` | row hash != 현재 hash (다시 바뀜) |

### 불변식

**row가 있고 변경 목록에 있는 파일은 `State.hashes`에 현재 hash가 있다.** 1단계부터 넘어온 "hash는 화면이 patch를 가져간 파일만 저장한다"를 봤음 체크한 파일까지 넓히는 것이 이것이다. 그러면 1단계의 hash 비교(`refresh_paths`, `too_many`일 때 `hashes`에 있는 경로만 남기기)가 체크한 파일에도 그대로 걸리고, 바뀌면 `worktree-changed`(scope `file`)가 나간다. UI는 이 event로 `files`와 `repos` query를 이미 무효화한다.

불변식을 지키는 자리:
- **열 때**: 현재 브랜치의 row를 읽고, 목록에 있는 row 파일의 hash를 계산한다. 앱이 꺼진 사이 바뀐 파일이 여기서 잡힌다.
- **`refresh_list`**: 바뀐 경로의 hash를 지운 뒤, 그중 row가 있는 파일은 다시 계산한다.
- **`refresh_base`**: hash를 모두 지운 뒤 row 파일을 다시 계산한다. 브랜치가 바뀌었으면 새 브랜치의 row를 읽는다.
- **`refresh_paths`**: 이미 hash를 다시 계산하므로 추가 작업이 없다.

hash 계산은 state 교체와 같은 규칙을 따른다: 계산을 모두 끝낸 뒤 state를 바꾼다.

### 새 API

- `files()`가 `FileEntry { ...ChangedFile, viewed: ViewedState }`를 돌려준다. `ChangedFile`에는 넣지 않는다 (`refresh_list`가 `ChangedFile` 비교로 바뀐 파일을 찾는다).
- `set_viewed(path, viewed, hash: Option<String>)`:
  - `viewed = true`: `hash`가 있으면 그것을, 없으면 지금 계산한 hash를 저장한다. 현재 hash도 `hashes`에 넣는다. 화면이 보낸 hash가 그사이 바뀐 diff의 것이면 바로 `changed`로 보인다. 사용자가 본 것을 기록하는 것이므로 의도한 동작이다.
  - `viewed = false`: row를 지운다.
  - 목록에 없는 파일이면 `UnknownFile`.
- `WorktreeSummary.unviewed_files`: 변경 파일 중 `viewed`가 아닌 수 (`none` + `changed`). 접힌 파일도 센다.

### 알려진 한계

- hash는 diff 전체(hunk header의 줄 번호 포함)에 대해 계산한다. fetch로 merge-base가 바뀌어 같은 파일의 다른 곳이 달라지면, 내 변경이 그대로여도 `changed`가 될 수 있다. 그대로 둔다.
- 체크한 파일이 많은 레포를 열 때 그 수만큼 `git diff`를 부른다. 파일당 약 10ms로 본다. 느리면 plan에서 batch를 검토한다.

## 3. Tauri interface

| 종류 | 이름 | 내용 |
|---|---|---|
| command | `list_files(worktree_id, view)` | 원소마다 `viewed: 'none' \| 'viewed' \| 'changed'` 추가 |
| command | `set_viewed(worktree_id, path, viewed, hash?)` | 신규. 반환값 없음 |
| command | `list_repos` | `WorktreeSummary.unviewedFiles` 추가 |
| event | `worktree-changed` | 그대로 |

`set_viewed`는 event를 보내지 않는다. UI가 성공 후 `['files', worktreeId]`와 `['repos']`를 무효화한다 (내 판단). 반응을 빠르게 하려고 `files` query 값을 먼저 바꿔 둔다 (optimistic update). 실패하면 되돌리고 오류를 보인다.

## 4. UI

```
┌───────────────────────┬──────────────────────┬──────────────────────────────────────────┐
│ omofictions-front   N │ omofictions-front · front/feat/1116 ← develop (PR)  확인 9/14 [split|unified] │
│   develop       0     ├──────────────────────┬───────────────────────────────────────────┤
│ ● front/feat/1116 5/14│ 변경 파일 14개        │ ▸ app/login/page.tsx        +40 -2  ☑ 봤음 │
│               3분 전  │ ▾ app/login          │ ▾ app/login/form.tsx  다시 바뀜 +12 ☐ 봤음 │
│                       │   ☑ page.tsx   +40   │   - old            │ + new                │
│                       │   ● form.tsx   +12   │                                           │
│                       │   ☐ util.ts    +3    │ ▾ app/login/util.ts         +3   ☐ 봤음   │
└───────────────────────┴──────────────────────┴───────────────────────────────────────────┘
```

- **diff header**: 오른쪽에 `봤음` checkbox. checkbox 클릭은 header의 접기 toggle로 번지지 않는다.
  - 체크하면 그 파일 diff를 접는다. 체크를 풀면 펼친다 (GitHub PR과 같은 동작, 내 판단).
  - `viewed`로 처음 그려지는 파일은 접힌 채로 시작한다. `changed`인 파일은 펼친 채로 시작하고, header에 amber `다시 바뀜` badge.
  - 이미 그려진 파일이 `viewed` → `changed`로 바뀌면 펼친다.
  - 화면이 patch를 가져갔으면 그 hash를 보내고, 아니면 hash 없이 보낸다.
- **file tree**: status 글자(A/M/D/U)는 그대로 두고 그 앞에 봤음 표시를 둔다: 봤음 ☑(회색), 다시 바뀜 ●(amber), 미확인은 빈칸. 봤음 파일 이름은 회색으로 흐리게 (내 판단).
- **header**: `확인 n/m`. n = `viewed` 수, m = 전체 변경 파일 수. 모두 봤으면 초록.
- **sidebar**: worktree 줄의 숫자를 `미확인/전체`로 바꾼다 (`5/14`). 미확인이 0이면 전체만 회색으로 (`14`) (내 판단).

## 5. 오류 처리와 경계 사례

- 체크한 파일이 목록에서 빠지면(revert) 화면에서 사라지고, row는 남는다.
- 브랜치 전환: HEAD 변경 → `refresh_base` → 새 브랜치 row로 계산. 돌아오면 이전 체크가 다시 보인다.
- detached HEAD는 branch `''`로 저장한다. detached 상태에서 다른 commit으로 옮겨도 같은 key다. hash가 다르면 `changed`로 보인다.
- DB 쓰기 실패: `set_viewed`가 오류를 돌려주고 UI는 optimistic update를 되돌린다.
- 같은 레포를 가리키는 앱 두 개를 띄우는 경우는 고려하지 않는다 (SQLite가 직렬화한다).

## 6. test

test를 먼저 쓰고 실제 git과 실제 FSEvents를 쓴다.

**core (`cargo test`)**
1. `ViewedStore`: set / load / remove, 다시 열어도 남음, 브랜치별로 분리.
2. `Worktree`: 체크 → `viewed`. 파일 수정 → `handle` 후 `changed`와 scope `file` event. 다시 체크 → `viewed`. 체크 해제 → `none`.
3. `+`/`-` 수가 같은 수정 (한 글자 바꾸기)도 `changed`가 된다 (hash 비교 경로).
4. 앱이 꺼진 사이 바뀜: 체크 → `Worktree` drop → 파일 수정 → 다시 열기 → `changed`.
5. `too_many` 경로 (200개 넘는 event)에서도 체크한 파일의 변경을 잡는다.
6. 브랜치 전환 후 되돌아오면 체크가 남아 있다.
7. hash 없이 체크 (patch를 안 가져온 파일) → 현재 hash로 저장.
8. watcher 통합: 체크한 파일을 고치면 1초 안에 event가 오고 `changed`.
9. `read_only.rs`: 체크 후에도 `.git/index` 불변.

**UI (`pnpm --dir ui test`)**: 확인 수 계산, 상태별 처음 접힘 여부, optimistic update와 실패 시 되돌리기.

**실제 UI 확인**: `scripts/fixtures.sh` fixture로 `pnpm tauri dev`를 띄워 screenshot으로 확인한다. 파일 체크 → 접힘과 `확인 n/m` → 터미널에서 그 파일 수정 → 1초 안에 `다시 바뀜` → 앱 재시작 후 체크 유지. 마지막에 실제 `~/.wtboard/config.toml`로 읽기만 하며 확인한다.
