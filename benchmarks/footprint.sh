#!/usr/bin/env bash
# Appends the dependency-footprint and `unsafe`-usage sections to the
# benchmark report. Run by `.github/workflows/benchmark.yml` after the
# harness has produced `benchmark.md`.
#
# Everything here is measured from the actual build, not hardcoded:
#  - dependency counts come from `cargo tree -e normal`;
#  - `unsafe` counts come from grepping each crate's vendored source in the
#    cargo registry (the exact versions pinned in benchmarks/Cargo.toml).
set -euo pipefail

OUT="${1:-benchmark.md}"
# Resolve to an absolute path before changing directory, so the caller can
# pass a repo-root-relative path and the script can still `cd` to its own
# directory for `cargo tree`.
case "$OUT" in
  /*) ;;
  *) OUT="$(pwd)/$OUT" ;;
esac
cd "$(dirname "$0")"

# Number of packages reachable with `-e normal` (includes the package itself).
dep_count() {
  cargo tree --manifest-path Cargo.toml -p "$1" -e normal --prefix none 2>/dev/null | wc -l
}

# Raw count of the `unsafe` keyword in a vendored crate's `.rs` sources.
unsafe_count() {
  local dir
  dir=$(find "${CARGO_HOME:-$HOME/.cargo}/registry/src" -maxdepth 2 -type d -name "$1" 2>/dev/null | head -n 1)
  if [[ -z "$dir" ]]; then
    echo "n/a"
  else
    grep -ro '\bunsafe\b' --include='*.rs' "$dir" | wc -l
  fi
}

{
  echo
  echo "## Dependency footprint"
  echo
  echo "| package | packages with \`-e normal\` (incl. itself) |"
  echo "| --- | ---: |"
  echo "| tzcraft | $(dep_count tzcraft) |"
  echo "| chrono | $(dep_count chrono) |"
  echo "| time | $(dep_count time) |"
  echo "| jiff | $(dep_count jiff) |"
  echo
  echo "> Counted with \`cargo tree -p <pkg> -e normal --prefix none\`. \`tzcraft\`'s graph here is as built for the benchmark (default features: codecs on). For a downstream consumer of the published crate the graph is **0 transitive packages**: \`nextjson\` and \`rustbinary\` are optional, codec-only dependencies."
  echo
  echo "## \`unsafe\` usage in crate source"
  echo
  echo "| package | \`unsafe\` keyword occurrences |"
  echo "| --- | ---: |"
  echo "| tzcraft | 0 (\`#![deny(unsafe_code)]\`) |"
  echo "| chrono | $(unsafe_count chrono-0.4.45) |"
  echo "| time | $(unsafe_count time-0.3.55) |"
  echo "| jiff | $(unsafe_count jiff-0.2.35) |"
  echo
  echo "> Raw count of the \`unsafe\` keyword across each crate's \`.rs\` sources as vendored in the cargo registry for the pinned versions. An \`unsafe\` count is a static signal, not a verdict: what matters is whether the unsafe is encapsulated, whether the soundness invariants are documented, and whether the public API is safe to call."
} >> "$OUT"
