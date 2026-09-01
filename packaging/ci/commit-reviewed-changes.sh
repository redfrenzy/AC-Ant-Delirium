#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)

fail() {
    printf '%s\n' "AD-COMMIT: $1" >&2
    exit 1
}

[[ ${1:-} == '--message' && -n ${2:-} && ${3:-} == '--' && $# -ge 4 ]] \
    || fail 'use --message <single-line-message> -- <explicit-relative-file>...'
message=$2
shift 3
paths=("$@")

[[ ${#message} -le 100 && "$message" != *$'\n'* && "$message" != *$'\r'* ]] \
    || fail 'the commit message must be one line with at most 100 characters.'
[[ -d "$repository_root/.git" && ! -L "$repository_root/.git" ]] \
    || fail 'the expected Git worktree is unavailable.'
[[ $(git -C "$repository_root" rev-parse --show-toplevel) == "$repository_root" ]] \
    || fail 'the script is not running in its reviewed repository.'

declare -A allowed_paths=()
for path in "${paths[@]}"; do
    case "$path" in
        ''|/*|./*|../*|*/../*|*/..|*//*|:*|.git|.git/*|target|target/*)
            fail 'every commit path must be an explicit safe project-relative file.'
            ;;
    esac
    [[ "$path" != *$'\n'* && "$path" != *$'\r'* ]] \
        || fail 'commit paths cannot contain line breaks.'
    allowed_paths["$path"]=1
done

cd "$repository_root"
git diff --check -- "${paths[@]}"
git add -A -- "${paths[@]}"
git diff --cached --check

staged_count=0
while IFS= read -r -d '' staged_path; do
    [[ -n ${allowed_paths["$staged_path"]+present} ]] \
        || fail 'the index contains a file outside the explicit commit set.'
    staged_count=$((staged_count + 1))
done < <(git diff --cached --name-only -z --diff-filter=ACMRD)
[[ $staged_count -gt 0 ]] || fail 'there are no reviewed changes to commit.'

git diff --cached --stat
git commit -m "$message"
git status --short
