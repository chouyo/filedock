#!/usr/bin/env bash
# Runs every search benchmark on one dataset, then prints the comparison.
#
#   examples/scripts/run-all.sh [size] [common options…]
#   examples/scripts/run-all.sh 100k
#   examples/scripts/run-all.sh 1m --repeat 3 --timeout 120
#   SEED=7 examples/scripts/run-all.sh 100k
#
# Datasets, index state and reports go to ~/FileDockBench (FILEDOCK_BENCH_HOME).
set -euo pipefail

SIZE="${1:-100k}"
shift || true
EXTRA=("$@")
SEED="${SEED:-42}"

cd "$(dirname "$0")/../.." # app/src-tauri
EXAMPLES=(bench_dataset bench_current bench_seekr bench_native bench_spotlight bench_compare)
build_args=()
for ex in "${EXAMPLES[@]}"; do build_args+=(--example "$ex"); done
cargo build --release "${build_args[@]}"
E=target/release/examples

DS="$("$E/bench_dataset" gen --size "$SIZE" --seed "$SEED")"
# Undo offline mutations even if a backend fails halfway.
trap '"$E/bench_dataset" revert --dataset "$DS" >/dev/null 2>&1 || true' EXIT
"$E/bench_dataset" revert --dataset "$DS"

# Full cycle: S1–S4 + S6, offline mutation, S5 + S6 in a new process, revert.
full() {
  local bin="$1"
  shift
  echo "=== $bin $* ===" >&2
  "$E/$bin" --dataset "$DS" --scenario all "$@" ${EXTRA[@]+"${EXTRA[@]}"} >/dev/null || echo "!! $bin failed" >&2
  "$E/bench_dataset" mutate --dataset "$DS"
  "$E/$bin" --dataset "$DS" --scenario s5,s6 "$@" ${EXTRA[@]+"${EXTRA[@]}"} >/dev/null || echo "!! $bin s5 failed" >&2
  "$E/bench_dataset" revert --dataset "$DS"
}

# Selected scenarios only (variants).
only() {
  local bin="$1" scenarios="$2"
  shift 2
  echo "=== $bin $* ($scenarios) ===" >&2
  "$E/$bin" --dataset "$DS" --scenario "$scenarios" "$@" ${EXTRA[@]+"${EXTRA[@]}"} >/dev/null || echo "!! $bin failed" >&2
}

# Spotlight first: right after generation its S1 measures how long the OS
# takes to index the dataset, and that background indexing would otherwise
# slow down the other backends.
if [[ "$(uname)" == "Darwin" ]]; then
  full bench_spotlight
fi
full bench_current
full bench_seekr
full bench_native
only bench_native s1 --walker walkdir
only bench_native s1,s3 --fts

# Level-3 resume needs its own index + mutation cycle.
echo "=== bench_native --resume-mode full ===" >&2
"$E/bench_native" --dataset "$DS" --scenario s1 ${EXTRA[@]+"${EXTRA[@]}"} >/dev/null
"$E/bench_dataset" mutate --dataset "$DS"
"$E/bench_native" --dataset "$DS" --scenario s5,s6 --resume-mode full ${EXTRA[@]+"${EXTRA[@]}"} >/dev/null || echo "!! resume-full failed" >&2
"$E/bench_dataset" revert --dataset "$DS"

"$E/bench_compare" --dataset "$DS"
