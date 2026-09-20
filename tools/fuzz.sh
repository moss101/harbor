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
