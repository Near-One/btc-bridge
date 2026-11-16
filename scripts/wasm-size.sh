#!/usr/bin/env bash
#
# Measure the release WASM size of the bridge contracts.
#
# Builds each requested contract with the release profile, prints a size table
# and, when given a baseline, the per-contract delta. Exits non-zero if an
# artifact grew past the allowed threshold, which is what makes it usable as a
# CI gate against binary-size regressions.
#
# Usage:
#   scripts/wasm-size.sh [options] [TARGET ...]
#
#   TARGET   bitcoin | zcash | nbtc | all   (default: bitcoin zcash)
#
# Options:
#   --out FILE         write the measured sizes to FILE (TSV: name<TAB>bytes)
#   --input FILE       read sizes from FILE instead of building anything
#   --baseline FILE    compare against sizes written by an earlier --out
#   --markdown FILE    append the table as markdown (e.g. $GITHUB_STEP_SUMMARY)
#   --threshold PCT    fail above PCT growth vs baseline  (default: 2)
#   --min-delta BYTES  growth below this never fails      (default: 1024)
#   --reproducible     build in Docker (`make build-<target>`) instead of the
#                      local toolchain; bit-for-bit sizes, much slower
#
# Env:
#   REPO_ROOT   repo to build in (default: the parent of this script's dir)
#
# Examples:
#   scripts/wasm-size.sh                                   # build + print sizes
#   scripts/wasm-size.sh --out head.tsv                    # record them
#   scripts/wasm-size.sh --input head.tsv --baseline base.tsv   # compare only
#
# Exit code 0 = nothing grew past the threshold.

set -euo pipefail

REPO_ROOT="${REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"

TARGETS=()
OUT=""
INPUT=""
BASELINE=""
MARKDOWN=""
THRESHOLD=2
MIN_DELTA=1024
REPRODUCIBLE=0

bold() { printf '\033[1m%s\033[0m\n' "$*"; }
bad()  { printf '\033[31m%s\033[0m\n' "$*"; }
die()  { bad "error: $*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --out)          OUT="${2:?--out needs a path}"; shift 2 ;;
    --input)        INPUT="${2:?--input needs a path}"; shift 2 ;;
    --baseline)     BASELINE="${2:?--baseline needs a path}"; shift 2 ;;
    --markdown)     MARKDOWN="${2:?--markdown needs a path}"; shift 2 ;;
    --threshold)    THRESHOLD="${2:?--threshold needs a number}"; shift 2 ;;
    --min-delta)    MIN_DELTA="${2:?--min-delta needs a number}"; shift 2 ;;
    --reproducible) REPRODUCIBLE=1; shift ;;
    -h|--help)      sed -n '2,/^$/p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*)             die "unknown option '$1'" ;;
    *)              TARGETS+=("$1"); shift ;;
  esac
done

[ ${#TARGETS[@]} -eq 0 ] && TARGETS=(bitcoin zcash)
[ "${TARGETS[0]}" = "all" ] && TARGETS=(bitcoin zcash nbtc)

# validated here and not in target_spec, which runs in a subshell where `die`
# would only end the subshell
for target in "${TARGETS[@]}"; do
  case "$target" in
    bitcoin|zcash|nbtc) ;;
    *) die "unknown target '$target' (expected: bitcoin, zcash, nbtc, all)" ;;
  esac
done

# target -> make goal | produced artifact, relative to the repo root
target_spec() {
  if [ "$REPRODUCIBLE" = "1" ]; then
    case "$1" in
      bitcoin) echo "build-bitcoin|res/bitcoin_bridge_release.wasm" ;;
      zcash)   echo "build-zcash|res/zcash_bridge_release.wasm" ;;
      nbtc)    echo "release-nbtc|res/nbtc_release.wasm" ;;
    esac
  else
    case "$1" in
      bitcoin) echo "build-local-bitcoin|res/bitcoin_bridge.wasm" ;;
      zcash)   echo "build-local-zcash|res/zcash_bridge.wasm" ;;
      nbtc)    echo "nbtc|res/nbtc.wasm" ;;
    esac
  fi
}

# ------------------------------------------------------------------- measuring
SIZES=$(mktemp "${TMPDIR:-/tmp}/wasm-size.XXXXXX")
trap 'rm -f "$SIZES"' EXIT

if [ -n "$INPUT" ]; then
  [ -f "$INPUT" ] || die "no such file: $INPUT"
  cp "$INPUT" "$SIZES"
else
  for target in "${TARGETS[@]}"; do
    IFS='|' read -r goal artifact <<< "$(target_spec "$target")"
    bold "==> Building '$target': make $goal"
    ( cd "$REPO_ROOT" && make "$goal" )
    [ -f "$REPO_ROOT/$artifact" ] || die "build produced no artifact at $artifact"
    # `wc -c` rather than `stat`, whose flags differ between Linux and macOS
    printf '%s\t%s\n' "$target" "$(wc -c < "$REPO_ROOT/$artifact" | tr -d ' ')" >> "$SIZES"
  done
fi

[ -s "$SIZES" ] || die "nothing measured"
[ -n "$OUT" ] && cp "$SIZES" "$OUT"

# ------------------------------------------------------------------- reporting
# awk renders both the plain and the markdown table, and exits 1 if any artifact
# grew by more than --threshold percent *and* more than --min-delta bytes.
render() {
  awk -v baseline="$BASELINE" -v threshold="$THRESHOLD" -v min_delta="$MIN_DELTA" \
      -v md="${1:-0}" '
    function kib(b) { return sprintf("%.1f KiB", b / 1024) }
    # small deltas read better as exact bytes than as a rounded KiB figure
    function signed(b,   a) {
      a = (b < 0 ? -b : b)
      return (b >= 0 ? "+" : "-") (a < 10240 ? sprintf("%d B", a) : sprintf("%.1f KiB", a / 1024))
    }
    BEGIN {
      if (baseline != "")
        while ((getline line < baseline) > 0) {
          split(line, f, "\t"); if (f[1] != "") base[f[1]] = f[2]
        }
      if (md) {
        print "### WASM size"
        print ""
        if (baseline != "") {
          print "| Contract | Size | Base | Δ | Δ % |"
          print "| --- | ---: | ---: | ---: | ---: |"
        } else {
          print "| Contract | Size | Bytes |"
          print "| --- | ---: | ---: |"
        }
      } else {
        printf "%-10s %12s %12s %12s %9s\n", "CONTRACT", "SIZE", "BASE", "DELTA", "DELTA %"
      }
    }
    {
      name = $1; size = $2 + 0
      if (baseline == "") {
        if (md) printf "| %s | %s | %d |\n", name, kib(size), size
        else    printf "%-10s %12s %12s %12s %9s\n", name, kib(size), "-", "-", "-"
        next
      }
      if (!(name in base)) {
        if (md) printf "| %s | %s | — | new | — |\n", name, kib(size)
        else    printf "%-10s %12s %12s %12s %9s\n", name, kib(size), "-", "new", "-"
        next
      }
      b = base[name] + 0
      delta = size - b
      pct = (b > 0) ? delta * 100.0 / b : 0
      over = (delta > min_delta && pct > threshold)
      if (over) { failed = 1; mark = " ⚠️"; flag = " <-- over threshold" } else { mark = ""; flag = "" }
      if (md) printf "| %s | %s | %s | %s | %+.2f%%%s |\n", name, kib(size), kib(b), signed(delta), pct, mark
      else    printf "%-10s %12s %12s %12s %+8.2f%%%s\n", name, kib(size), kib(b), signed(delta), pct, flag
    }
    END {
      if (md && baseline != "") {
        print ""
        printf "Fails above +%s%% growth (ignoring deltas under %d bytes).\n", threshold, min_delta
      }
      exit failed
    }
  ' "$SIZES"
}

echo
status=0
render 0 || status=$?

if [ -n "$MARKDOWN" ]; then
  # appended, so several runs can stack up in one job summary
  render 1 >> "$MARKDOWN" || true
fi

if [ "$status" -ne 0 ]; then
  echo
  die "WASM size regression: growth above ${THRESHOLD}% (see table above)"
fi

exit 0
