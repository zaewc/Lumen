#!/usr/bin/env bash
set -euo pipefail

staged_rust_files=()
while IFS= read -r -d '' file; do
  staged_rust_files+=("$file")
done < <(git diff --cached --name-only --diff-filter=ACMR -z -- '*.rs')

if ((${#staged_rust_files[@]} == 0)); then
  exit 0
fi

if ! git diff --quiet -- "${staged_rust_files[@]}"; then
  printf '%s\n' \
    "Rust formatting stopped: a staged Rust file also has unstaged changes." \
    "Stage or separate those changes, then retry the commit."
  exit 1
fi

rustfmt --edition 2024 --config-path rustfmt.toml \
  --config skip_children=true "${staged_rust_files[@]}"
git add -- "${staged_rust_files[@]}"
