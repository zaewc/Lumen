#!/usr/bin/env bash
# Whether a pull request changes anything besides documentation, so CI can
# skip building and testing a documentation-only change. Prints `code=true` or
# `code=false`, and appends it to $GITHUB_OUTPUT when that is set.
#
# Documentation is anything under docs/ and any Markdown file outside ai/
# (ai/ holds the committed Jev reports that replay must reproduce). Anything
# else, an empty diff, or anything but a pull request counts as code: when in
# doubt, run everything.
#
# The build and test workflows run it in a `changes` job and then skip their
# expensive steps, never the workflow itself: a workflow filtered out by
# `paths` never reports its required checks, and the pull request would wait
# forever. A matrix job skips its steps rather than the whole job, because a
# skipped matrix job reports one unexpanded name instead of one per entry.
#
# On a pull request, actions/checkout checks out the merge commit, whose first
# parent is the base branch, so `HEAD^1..HEAD` is what the pull request changes.
# The checkout needs `fetch-depth: 2`.
set -euo pipefail

code=true
if [ "${EVENT:-}" = pull_request ]; then
  changed=$(git diff --name-only HEAD^1 HEAD)
  if [ -n "$changed" ]; then
    code=false
    while IFS= read -r path; do
      case "$path" in
        ai/*) code=true ;;
        docs/* | *.md) ;;
        *) code=true ;;
      esac
    done <<<"$changed"
  fi
fi

echo "code=$code"
if [ -n "${GITHUB_OUTPUT:-}" ]; then
  echo "code=$code" >>"$GITHUB_OUTPUT"
fi
