#!/usr/bin/env bash
# Compares the benchmarks of every benchmarked crate a pull request changes,
# its base against its merge commit, on one runner, and writes criterion's
# medians and changes as a table to $GITHUB_STEP_SUMMARY (stdout when unset).
#
# Informational only: shared runners vary by several percent between runs, so
# only large changes mean anything (docs/architecture/performance.md). A
# benchmark new in the pull request has no base and reports `new`.
#
# On a pull request, actions/checkout checks out the merge commit, whose first
# parent is the base branch; the checkout needs `fetch-depth: 2`. A change to
# Cargo.lock, this script or its workflow runs every crate's benchmarks.
set -euo pipefail

crates=(lumen-cluster lumen-scan lumen-policy)
changed=$(git diff --name-only HEAD^1 HEAD)
selected=()
if grep -qxE 'Cargo\.lock|scripts/bench-compare\.sh|\.github/workflows/bench-compare\.yml' <<<"$changed"; then
  selected=("${crates[@]}")
else
  for crate in "${crates[@]}"; do
    if grep -q "^crates/$crate/" <<<"$changed"; then
      selected+=("$crate")
    fi
  done
fi

summary=${GITHUB_STEP_SUMMARY:-/dev/stdout}
if [ "${#selected[@]}" -eq 0 ]; then
  echo "No benchmarked crate changed." >>"$summary"
  exit 0
fi

# One target directory, so criterion keeps the base's results beside the head's.
export CARGO_TARGET_DIR="$PWD/target"
base=$(mktemp -d)
git worktree add --detach "$base/tree" HEAD^1

# The rows of one crate's `cargo bench` output: name, median, change, verdict.
table() {
  awk '
    function row() {
      if (name != "") print "| " name " | " time " | " change " | " verdict " |"
      name = ""
    }
    /^Benchmarking/ { next }
    /time:/ {
      row()
      name = $0; sub(/^ */, "", name); sub(/ *time:.*/, "", name)
      if (name == "") name = prev
      t = $0; sub(/.*time: *\[/, "", t); split(t, a, " ")
      time = a[3] " " a[4]; change = "new"; verdict = ""
      next
    }
    /change:/ && name != "" {
      c = $0; sub(/.*change: *\[/, "", c); split(c, b, " "); change = b[2]
      next
    }
    /Performance has improved/ { verdict = "faster" }
    /Performance has regressed/ { verdict = "slower" }
    /No change in performance|Change within noise/ { verdict = "no change" }
    /peak .* above the input/ { row(); line = $0; sub(/^ */, "", line); print "| " line " | | | |" }
    !/^ *$/ { prev = $0; sub(/^ */, "", prev) }
    END { row() }
  '
}

{
  echo "## Benchmarks: base → this pull request"
  echo
  echo "Same runner, one run each. Shared runners vary by several percent; read"
  echo "only large changes. Memory lines are this pull request's."
  echo
} >>"$summary"

for crate in "${selected[@]}"; do
  echo "::group::$crate (base)"
  if ! (cd "$base/tree" && cargo bench --locked -p "$crate" --benches -- --save-baseline base); then
    echo "The base did not run benchmarks for $crate; see above. Changes are shown without a base."
  fi
  echo "::endgroup::"
  echo "::group::$crate (this pull request)"
  cargo bench --locked -p "$crate" --benches -- --baseline-lenient base | tee "$base/$crate.log"
  echo "::endgroup::"
  {
    echo "### $crate"
    echo
    echo "| Benchmark | Median | Change | Verdict |"
    echo "| --- | ---: | ---: | --- |"
    table <"$base/$crate.log"
    echo
  } >>"$summary"
done

git worktree remove --force "$base/tree"
