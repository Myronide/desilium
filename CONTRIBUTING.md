# Contributing to Desilium

Thank you for taking the time to contribute. This guide describes how changes
move from an idea to the `main` branch, which checks they must pass, and the
style rules the code is held to.

Before you start, read [`concept.md`](concept.md) (goals and principles) and
[`docs/architecture.md`](docs/architecture.md) (crate and process boundaries).
Desilium runs on KWin and ships no compositor; if your change touches KWin or KDE
settings, protocols or shortcuts, also read
[`docs/kwin-integration.md`](docs/kwin-integration.md).

## Workflow

1. Open an issue or discussion for anything larger than a small fix, so the
   design can be agreed before code is written.
2. Create a topic branch from `main`.
3. Keep commits focused. One logical change per commit is easier to review.
4. Run the local checks (see below) until they pass.
5. Open a pull request. Describe *what* changed and *why*, and link the issue.
   For user-visible behaviour, say how you tested it.
6. CI must be green before review. Address review comments with new commits;
   squash or rebase only when asked.

Stay inside the crate you are changing. Changes that cross crate boundaries
(new dependencies between crates, new protocol surfaces, new configuration
keys) need a short design note in the pull request description.

## Running the checks

```sh
scripts/check.sh            # rustfmt, clippy, tests, Pureline (if installed)
scripts/check.sh --quick    # the same without the test suite
```

The script runs every step even if one fails and prints a summary at the end.
CI runs the same Rust checks as separate jobs (`fmt`, `clippy`, `test`) and
builds `docker/Dockerfile`. Run `scripts/check.sh` before pushing; it catches
almost everything CI would.

To run a single crate's tests:

```sh
cargo test -p desilium-kwin
```

See [`docs/testing.md`](docs/testing.md) for the test layers, what is not covered
(nothing runs against a live KWin automatically) and the Docker safety rules.

### Changes that touch KWin

- Keep KDE-specific knowledge (configuration keys, D-Bus names, protocol versions, action
  names) in `crates/desilium-kwin` and `crates/desilium-shell/src/services/protocol/`.
- A new setting that KWin should honour needs an entry in `crates/desilium-kwin/src/plan/`, a unit
  test, and a row in the mapping table of `docs/kwin-integration.md`.
- Anything taken from memory rather than checked against a running KWin goes into the
  "Verification status" list of that document. When you check an item, record the KWin version
  and change its status.
- Tests must never run the real `kwriteconfig6` or change the developer's KDE configuration.
- Run a nested session with `XDG_CONFIG_HOME` pointing to a throw-away directory, or in the
  container (`scripts/docker-run.sh`), because the bridge writes KDE configuration files.

## Pureline style

All Rust code follows **Pureline 1.5**, the style specification in
[`PURELINE_SPECIFICATION_1.5.md`](PURELINE_SPECIFICATION_1.5.md) (Rust rules
are `PL-RS-*`). The rules below are the ones contributors trip over most often.
Read the specification when a case is unclear.

| # | Rule | What it means in practice | Rule IDs |
|---|------|---------------------------|----------|
| 1 | Guard clauses | Validate inputs first and return early. The success path stays at the base indentation. | PL-RS-002, PL-CF-001 |
| 2 | No `else` after a returning branch | Write `let ... else` or an early `return` instead of `if ... { return } else { ... }`. | PL-RS-002, PL-CF-002 |
| 3 | No `unwrap`, `expect`, `panic!` in production code | Return `Result` or `Option` and propagate with `?`. Tests may unwrap. | PL-RS-004, PL-RS-003 |
| 4 | Small functions | One responsibility per function. Functions normally stay under about 40 lines. | PL-FN-001, PL-STRUCT-001 |
| 5 | Meaningful names | Names say what a thing is for. Single-letter names only for `i`, `j`, `k`, `x`, `y`. | PL-RS-001, PL-NAME-001 |
| 6 | Folder density | At most 10 hand-maintained `.rs` files per folder. Split along real responsibilities, not into empty sub-folders. | PL-RS-010 |
| 7 | Cohesion over size | A long `impl` or module is fine when it is one cohesive thing. Split only on a real responsibility boundary. | PL-RS-009, PL-STRUCT-003 |
| 8 | Isolated `unsafe` | `unsafe` lives in a tiny wrapper, with a `// SAFETY:` comment directly above it that states the invariant. FFI and platform code stays in dedicated modules. | PL-RS-015, PL-RS-016 |
| 9 | Comments explain why | Do not restate the code. Document the reason, the invariant or the trade-off. | PL-DOC-001 |
| 10 | Test the invalid paths | Every feature needs tests for the error and edge cases, not only the happy path. Tests must be deterministic: pass time in, do not sleep. | PL-RS-020 |

Several of these rules are enforced by clippy through the workspace lints in `Cargo.toml`
(`unwrap_used`, `expect_used`, `panic`, `redundant_else`, `manual_let_else` and rustc's
`unreachable_pub`) and `clippy.toml` (tests may unwrap). Items that are only used inside their
crate are `pub(crate)` or narrower (PL-RS-001). Integration test files may carry one
`#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` with a reason comment.

Also expected, although the checker does not flag all of it: `cargo fmt` is
the formatting authority, clippy runs with `-D warnings`, and clippy lints are
not silenced with blanket `#[allow(...)]` groups. A single, specific
`#[allow(lint)]` with a reason comment is acceptable.

### Running the Pureline checker

The checker is a separate tool that lives outside this repository. If it is
installed, `scripts/check.sh` finds it on `PATH` or at
`$HOME/tools/bin/pureline` and runs it. You can also run it directly:

```sh
pureline --no-info crates docs      # production code and docs (spikes/ are throwaway experiments and exempt)
pureline --list-rules           # every rule and its ID
```

Exit code 0 means no errors or warnings. Informational notices are fine.

### Suppressing a finding

Prefer fixing the code. When the finding is a false positive, silence exactly
one rule with a comment on the **same line or the line directly above** the
flagged code, and give a reason:

```rust
// pureline:allow(PL-RS-004) PALETTE is a non-empty const; a unit test asserts it
let first = PALETTE.first().expect("palette is non-empty");
```

Rules for suppressions:

- Name one rule ID per comment. No blanket suppressions.
- Keep the flagged call on a single line, so the comment applies to it.
- Always write the reason. Reviewers will ask for one if it is missing.

## Definition of done

A Desilium feature is complete only when all of the following hold
(from [`concept.md`](concept.md), section 11):

- It works reliably.
- It respects the modular architecture.
- It provides sensible defaults.
- It is configurable through the GUI where applicable.
- It does not require manual configuration editing.
- It handles invalid input.
- It provides meaningful feedback.
- It supports safe recovery where necessary.
- It has appropriate automated tests.
- Its performance has been evaluated.

The non-negotiable principle behind this list: **users must never be required
to manually modify configuration files to use, personalize, maintain, or
troubleshoot Desilium.** A pull request that adds a setting must also add the
way to change it in the graphical settings, or explain why that is not
applicable yet.

## Licensing

By contributing, you agree that your contributions are licensed under the MIT
License in [`LICENSE`](LICENSE).
