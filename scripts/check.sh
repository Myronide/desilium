#!/usr/bin/env bash
# Run the same checks as CI, locally, and summarise the result.
#
# Usage:
#   scripts/check.sh            # fmt, clippy, tests, Pureline
#   scripts/check.sh --quick    # fmt, clippy, Pureline (no tests)
#
# Every step runs even when an earlier one fails, so one invocation reports
# all problems. The exit status is non-zero if any step failed.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(dirname -- "$SCRIPT_DIR")"
PURELINE_FALLBACK="$HOME/tools/bin/pureline"

quick=0
results=()
failures=0

usage() {
    cat <<'EOF'
Usage: scripts/check.sh [--quick]

  --quick   skip the test suite (fmt, clippy and Pureline still run)
  -h        show this help
EOF
}

die() {
    printf 'check.sh: %s\n' "$*" >&2
    exit 2
}

section() {
    printf '\n==== %s ====\n' "$1"
}

# Runs one named step and records PASS or FAIL. Never aborts the script.
run_step() {
    local name="$1"
    shift
    section "$name"
    if "$@"; then
        results+=("PASS  $name")
    else
        results+=("FAIL  $name")
        failures=$((failures + 1))
    fi
}

record_skip() {
    results+=("SKIP  $1")
}

# Prints the path of an available Pureline binary, or fails if there is none.
find_pureline() {
    if command -v pureline >/dev/null 2>&1; then
        command -v pureline
        return 0
    fi
    if [[ -x "$PURELINE_FALLBACK" ]]; then
        printf '%s\n' "$PURELINE_FALLBACK"
        return 0
    fi
    return 1
}

print_summary() {
    local line
    section "Summary"
    for line in "${results[@]}"; do
        printf '  %s\n' "$line"
    done
    if ((failures > 0)); then
        printf '\n%d step(s) failed.\n' "$failures"
        return 1
    fi
    printf '\nAll executed checks passed.\n'
}

parse_args() {
    local arg
    for arg in "$@"; do
        case "$arg" in
            --quick) quick=1 ;;
            -h | --help)
                usage
                exit 0
                ;;
            *)
                usage >&2
                die "unknown argument: $arg"
                ;;
        esac
    done
}

main() {
    local pureline_bin
    parse_args "$@"
    command -v cargo >/dev/null 2>&1 || die "'cargo' not found in PATH (install Rust via rustup)"
    cd -- "$REPO_ROOT"

    run_step "rustfmt (cargo fmt --check)" cargo fmt --all --check
    run_step "clippy (warnings are errors)" cargo clippy --workspace --all-targets -- -D warnings

    if ((quick)); then
        record_skip "tests (--quick)"
    else
        run_step "tests (cargo test)" cargo test --workspace
    fi

    if pureline_bin="$(find_pureline)"; then
        run_step "Pureline style check" "$pureline_bin" --no-info crates docs
    else
        printf '\nnotice: pureline not found on PATH or at %s; style check skipped.\n' "$PURELINE_FALLBACK"
        record_skip "Pureline style check (binary not installed)"
    fi

    print_summary
}

main "$@"
