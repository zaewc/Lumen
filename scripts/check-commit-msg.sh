#!/usr/bin/env bash
# commit-msg hook: enforce Conventional Commits subjects and forbid attribution
# trailers (ADR-0027). Usage: check-commit-msg.sh <commit-message-file>
set -euo pipefail

msg_file="$1"
subject=$(head -n 1 "$msg_file")

types='feat|fix|docs|test|refactor|perf|build|ci|chore|revert'
re="^(${types})(\([a-z0-9][a-z0-9-]*\))?!?: [a-z0-9\`].{1,90}$"

if ! [[ "$subject" =~ $re ]]; then
  echo "error: commit subject must match 'type(scope): summary'" >&2
  echo "  types: ${types//|/, }; lowercase summary; at most ~100 characters" >&2
  echo "  got:   $subject" >&2
  exit 1
fi

if grep -qiE '^co-authored-by:' "$msg_file"; then
  echo "error: Co-Authored-By and other attribution trailers are not allowed (ADR-0027)" >&2
  exit 1
fi
