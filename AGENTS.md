# AGENTS.md

This file covers the repository. Read any closer AGENTS.md when working in a subdirectory.

## Project

Command Shelf is a Rust terminal UI for Herdr. Each tab owns a saved command list and can have one open shelf. Its bound shell must be in the same tab. Herdr starts the executable through actions and popup entrypoints in `herdr-plugin.toml`.

## Commands

<!-- AGENTS-GENERATED:START commands -->
- Build a linked checkout with `cargo build --release --locked`, then run `herdr plugin link .`.
- Run `cargo fmt --check`, `cargo test --locked`, and `cargo clippy --locked --all-targets -- -D warnings`.
- Exercise the release installer with `python3 tools/test_install.py` on Unix or `.\tools\test_install.ps1` in Windows PowerShell.
<!-- AGENTS-GENERATED:END commands -->

## Response Style

Lead with the result. Identify the host and distinguish native tests, emulated tests and cross-compilation when reporting compatibility.

## Workflow

1. Read the relevant Rust module, manifest entry and README section before editing a user-facing action.
2. Run the affected checks. For terminal changes, use a linked checkout and disposable Herdr state to exercise keyboard, mouse and popup behavior.
3. Report any platforms or interactive paths still awaiting verification.

## Golden Samples

- `src/herdr.rs` checks the bound terminal immediately before sending a saved command.
- `src/state.rs` locks list mutations, compares the confirmed entry and replaces JSON files atomically. The tab migration test in `src/main.rs` covers existing data.
- `tools/test_install.py` and `tools/test_install.ps1` use fixture downloads to exercise the release installation path.

## Heuristics (quick decisions)

<!-- AGENTS-GENERATED:START heuristics -->
- Register public actions in the manifest and Rust dispatcher. Keep IDs, binding defaults, and README examples in sync.
- Use argument vectors for Herdr CLI calls. The shelf launcher is typed into an interactive shell and requires platform-specific quoting. Pass each saved command as one argument to `herdr pane run`.
- Use the existing theme palette and button rendering for shelf and popup controls.
- Keep the Unix and Windows popup entries paired when changing a dialog. Match their IDs to the names constructed in `herdr::dialog`.
<!-- AGENTS-GENERATED:END heuristics -->

## Boundaries

### Always Do

Preserve per-tab ownership, explicit target binding and the checks for reported agents. Keep installer asset names, version tags, checksums and release matrix targets aligned.

### Ask First

Clarify ambiguous requests that would discard saved commands or silently redirect execution to another shell.

### Never Do

Require Cargo for a release install, persist real user state in fixtures, or interpolate a saved command into the shelf launcher.

## Contracts this code depends on

| Contract | Consumer |
| --- | --- |
| Herdr supplies the plugin state directory, focused-pane context and pane identity. The session socket path scopes tab storage. | `src/main.rs`, `src/herdr.rs`, `src/state.rs` |
| Pane get/list results include tab, terminal and agent state. Successful pane run may have empty stdout. | `src/herdr.rs` |
| Herdr reports the config path in its help output, accepts a candidate via HERDR_CONFIG_PATH, and can reload validated configuration. | `src/bindings.rs` |
| Popup entrypoints receive COMMAND_SHELF_DIALOG with the tab and action payload. | `herdr-plugin.toml`, `src/herdr.rs`, `src/main.rs` |
| Release assets use the manifest version and Rust target triple, a Windows .exe suffix, and a companion .sha256 file with a hex digest. | `scripts/install.sh`, `scripts/install.ps1`, `.github/workflows/release.yml` |

<!-- AGENTS-GENERATED:START module-boundaries -->
Keep dispatch in `src/main.rs`, Herdr calls in `src/herdr.rs`, persistence in `src/state.rs`, shortcut installation in `src/bindings.rs`, palette resolution in `src/theme.rs`, and terminal interaction in `src/ui.rs`.
<!-- AGENTS-GENERATED:END module-boundaries -->

## Scoped AGENTS.md

<!-- AGENTS-GENERATED:START scope-index -->
The root file currently covers every directory.
<!-- AGENTS-GENERATED:END scope-index -->

## When instructions conflict

Follow explicit user instructions. Reconcile differences between the manifest, implementation, tests and README against the requested behavior.
