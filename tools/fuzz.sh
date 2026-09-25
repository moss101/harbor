#!/usr/bin/env bash
# Run the cargo-fuzz targets (production plan C4) for a bounded time each.
#
#   tools/fuzz.sh [seconds-per-target] [target ...]
#
# Needs a nightly toolchain and cargo-fuzz (`rustup toolchain install
# nightly --profile minimal && cargo install cargo-fuzz`). Sanitizers are
# off (`-s none`): the property under test is "never panics, never aborts,
# always answers"; Harbor's unsafe surface is the C boundary only. The
# extra link flag lets the harbor_ffi cdylib link under the sancov
# instrumentation cargo-fuzz applies to every crate (its
# __sanitizer_cov_* symbols live in the fuzz binary, not the dylib).
# Findings land in core/fuzz/artifacts/<target>/; the corpus under
# core/fuzz/corpus/<target>/ grows and is committed after review.
#
# WARNING: `cargo fuzz cmin <target>` REPLACES that directory with the
# coverage-minimal set. It does not know which files were curated, so it
# deletes tracked seeds — including named ones like `docx_replace` and
# `garbage`, and any crash reproducer committed as a regression seed —
# whenever some other input happens to cover the same edges. Run it, then
# `git checkout -- core/fuzz/corpus/` to bring the reviewed seeds back
# before committing anything.
#
# Coverage is very uneven and the budget here does not account for it.
# Measured at 240 s per target on the qualification Mac:
#   ffi_dispatch        3,722 runs   (~15/s)
#   jsonschema      2,572,614 runs
#   batch_from_value 2,168,332 runs
#   graph_from_value 1,884,661 runs
# ffi_dispatch does real SQLite work per iteration, which is correct —
# it is the C boundary and the point is to exercise the real path — but
# CI's 60 s gives it roughly 900 executions against the others' hundreds
# of thousands.
#
# It also starts from few seeds, and that part is DELIBERATE, not an
# oversight: core/fuzz/.gitignore ignores `corpus/*/` entries whose names
# are exactly 40 characters, which is precisely libFuzzer's SHA-1-named
# output. Curated seeds with readable names are tracked; machine-grown
# ones stay local, and the repo stays lean. The cost is that a slow
# target re-derives coverage every CI run. A minimised ffi_dispatch
# corpus is ~486 files / 1.9 MB covering 20,334 edges — force-adding it
# would trade repo weight for a much more effective 60 s, which is a
# judgement about this repo rather than a bug to fix.
set -euo pipefail
seconds="${1:-60}"
shift || true
targets=("$@")
if [[ ${#targets[@]} -eq 0 ]]; then
  targets=(ffi_dispatch jsonschema batch_from_value graph_from_value)
fi
cd "$(dirname "$0")/../core/fuzz"
# `cargo +nightly` needs the rustup shim, not a Homebrew cargo that may
# shadow it on this machine.
if [[ -x "$HOME/.cargo/bin/cargo" ]]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi
if [[ "$(uname -s)" == "Darwin" ]]; then
  # Mach-O refuses undefined symbols in a dylib; ELF allows them by default.
  export RUSTFLAGS="${RUSTFLAGS:-} -C link-args=-Wl,-undefined,dynamic_lookup"
fi
status=0
for t in "${targets[@]}"; do
  echo "== fuzz $t for ${seconds}s =="
  if ! cargo +nightly fuzz run -s none "$t" -- -max_total_time="$seconds" -timeout=20 -rss_limit_mb=4096 2>&1 | tail -n 12; then
    echo "!! $t found a crash or timed out (see core/fuzz/artifacts/$t/)"
    status=1
  fi
done
exit $status
