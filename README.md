# Command Shelf

Save commands for a Herdr tab and run them in a shell you choose. Each tab has its own list, one shelf, and one bound shell. Clicking a command runs it with that shell’s current directory and environment.

[![Herdr plugin](https://img.shields.io/badge/herdr-plugin-76e6a3?style=for-the-badge)](https://herdr.dev/plugins)
[![Requires Herdr 0.9.5 or newer](https://img.shields.io/badge/herdr-%E2%89%A50.9.5-6db8ff?style=for-the-badge)](https://herdr.dev/docs/plugins/)
![Linux, macOS and Windows](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-f2c66d?style=for-the-badge)
[![MIT license](https://img.shields.io/badge/license-MIT-blue?style=for-the-badge)](LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/kormic911/herdr-command-shelf/release.yml?branch=main&label=CI&style=for-the-badge)](https://github.com/kormic911/herdr-command-shelf/actions/workflows/release.yml)

## Requirements

| Host | Release binary | Installer requirements |
| --- | --- | --- |
| Linux x64 or ARM64 | Static musl executable for the host architecture | Bash, curl, and either sha256sum or shasum |
| macOS on Intel or Apple Silicon | Native executable for each architecture | Bash, curl, and shasum or sha256sum |
| Windows x64 | x64 MSVC executable | Windows PowerShell 5.1 |
| Windows ARM64 | Native ARM64 MSVC executable | Windows PowerShell 5.1 |

All hosts require Herdr 0.9.5 or newer. On Windows, the shelf can start from a PowerShell or cmd prompt.

## Install

```sh
herdr plugin install kormic911/herdr-command-shelf --yes
```

The installer downloads the prebuilt binary for your platform and verifies its SHA-256 checksum. You can install Command Shelf without Rust.

## First run

1. Run **Install Command Shelf key bindings** from Herdr’s plugin actions.
2. Focus a spare shell prompt, for example one you created by splitting Notes down. Press the Herdr prefix (**Ctrl+B** by default), release it, then press **F2**. The pane becomes **Command Shelf**. If the tab already has an open shelf, this focuses it.
3. Focus the shell where commands should run. Press the prefix, release it, then press **F3**. Check **Run in** on the shelf for the pane title or ID. Changing focus preserves the binding.
4. In the shelf, press **n**, enter an optional title, and paste the command into the multiline field. Click **Save**. Click the saved entry to run it. The shelf stays open.

## Actions

| Herdr action ID | Invoke from | Effect |
| --- | --- | --- |
| `install-keybindings` | Anywhere | Add missing shortcuts, validate the configuration and reload Herdr. |
| `open` | A spare shell prompt | Open or focus this tab’s shelf. |
| `bind` | The destination shell in that tab | Set the target for subsequent runs. |

## Key bindings

| Where | Keys | Action |
| --- | --- | --- |
| Herdr | Prefix then **F2** / **F3** | Open shelf / bind focused shell |
| Shelf | **↑** / **↓**, then **Enter** | Select a command and run it |
| Shelf | **n** / **e** / **d** | New / edit / delete selected command |
| Shelf | **Esc** or **q** | Return to the original shell |
| New or Edit popup | **Tab** / **Shift+Tab** | Move between fields and Save |
| New or Edit popup | **s** or **Enter** with Save focused | Save the command |
| New or Edit popup | **Space** with the checkbox focused | Toggle **Require confirmation to run** |
| New, Edit, or Confirm Run | **↑** / **↓**, **Page Up** / **Page Down**, or mouse wheel over the command | Scroll through a multiline command (focus the command for keyboard scrolling in New or Edit) |
| Confirm Run popup | **y** / **Esc** | Run / cancel |
| Delete popup | **y** | Confirm deletion |
| Any popup | **Esc** | Cancel |

The footer buttons are clickable. Long and multiline commands display a one-line preview with an ellipsis. Edit shows the full text. Enable **Require confirmation to run** in New or Edit to review the full command and its destination each time you run it. Click **Run** to send it or **Cancel** to return to the shelf. The checkbox defaults to off for new and existing commands. New, Edit, and Delete use floating popups centered over the workspace.

## Configuration

The binding installer uses the configuration path reported by `herdr --help`. It preserves existing assignments and reports conflicts in the plugin log. To change a shortcut, edit its `[[keys.command]]` entry in Herdr’s configuration.

The shelf uses Herdr’s workspace selection colors and Settings button colors. It reads saved theme changes while open. With automatic theme switching, reopen the shelf after changing the terminal’s light or dark appearance.

## Driving the plugin from scripts

```sh
herdr plugin action invoke install-keybindings --plugin herdr-command-shelf
herdr plugin action invoke herdr-command-shelf.open
herdr plugin action invoke herdr-command-shelf.bind
```

Open and Bind use the focused pane when invoked. From a checkout, run `./scripts/install-keybindings.sh` or `.\scripts\install-keybindings.ps1` to see binding installer output directly.

## State and cleanup

Commands and the target live under `HERDR_PLUGIN_STATE_DIR/tabs/`, keyed by Herdr session and tab ID. Renaming the tab, rebinding, or closing and reopening the shelf preserves the list. Use **d** to review and delete an entry.

### Finding and editing a tab’s files

| Platform | Default plugin state directory |
| --- | --- |
| Windows | `%LOCALAPPDATA%\herdr\plugins\herdr-command-shelf` |
| Linux and macOS | `~/.local/state/herdr/plugins/herdr-command-shelf` |

If Herdr uses `XDG_STATE_HOME`, start in `$XDG_STATE_HOME/herdr/plugins/herdr-command-shelf` instead. Inside the plugin state directory, each tab has a `tabs/<session-hash>/<tab-id-in-hex>/` folder. The session hash is the SHA-256 of the session socket path. Use `herdr tab list` to find the tab ID. For example, `w1:t1` is stored in `77313a7431`. To locate candidate files, run `Get-ChildItem .\tabs -Filter commands.json -Recurse` in PowerShell or `find tabs -name commands.json` on Linux and macOS. Check the saved titles to identify the shelf.

**commands.json** holds entries in display order. To edit by hand, close the shelf and its dialogs, edit the file as UTF-8 JSON without a byte-order mark, then reopen the shelf.

```json
[
  {"title": "Run tests", "command": "cargo test"},
  {"title": "Build and publish", "command": "cargo build --release\ncargo publish", "require_confirmation": true}
]
```

Use an empty string for an untitled command and `\n` for a line break. Omitting `require_confirmation` defaults it to `false`. An empty list is `[]`.

**target.json** holds the bound shell, for example `{"pane":"w1:p2","terminal":"term_2"}`. Both IDs identify the destination. Use the Bind action to write the current pair. A shelf with no binding can have no `target.json`.

### Upgrading pane-owned shelves

When upgrading from pane-owned shelves, quit the old shelves and leave their shells open. On first open in each tab, Command Shelf imports entries from those terminals. It prefers the opening pane’s saved binding, then the first available binding. Original files remain in place. Old data for closed terminals needs manual recovery because it contains no tab ID.

## Trust and security

Command Shelf stores commands as plain text. For credentials, use references that your shell resolves at execution time.

Run checks the tab, pane and terminal identity and blocks input to a reported agent, including an idle agent. Herdr checks status and sends input in separate calls. A foreground program can receive the command if it takes over the target. Bind a shell you intend to use for commands.

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| Shortcut does nothing | Run the binding installer and check for a skipped chord with `herdr plugin log list --plugin herdr-command-shelf --limit 3`. Use the configured prefix followed by the function key. |
| Run is disabled | Focus a shell prompt in the same tab and bind it. A closed or replaced terminal requires an explicit rebind. |
| Release download returns 404 | Check that the release for the version in `herdr-plugin.toml` includes the binary and checksum for your platform. |
| Windows build or update reports a locked executable or LNK1104 | Close all shelf panes and dialogs using that binary, then retry. |

## Development

```sh
cargo build --release --locked
herdr plugin link .
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

The linked checkout uses the binary in `target/release/`. To opt into building from source during plugin installation, set `HERDR_COMMAND_SHELF_BUILD_FROM_SOURCE=1` in the installer environment.

Run `python3 tools/test_install.py` on Unix and `.\tools\test_install.ps1` in Windows PowerShell to test installation with fixture downloads, including rejection of a corrupt asset. For an interactive check, open the shelf, bind another shell, save and run a command, then cancel and confirm a deletion. Close the target and verify that Run stays disabled until you bind again.

The workflow builds and smoke-tests all six release binaries on their native runners. Pull requests and pushes to main run checks. Manual runs upload build artifacts. To publish, align the versions in `Cargo.toml`, `Cargo.lock`, and `herdr-plugin.toml`, then push the corresponding version tag (for example, `v1.0.1`). The tag workflow publishes the binaries and their checksums. [Build and release workflow](.github/workflows/release.yml)

## License

Command Shelf uses the MIT license. The release installers, installer fixtures, and packaging workflow are adapted from herdr-notes. [License](LICENSE) · [herdr-notes notice](scripts/LICENSE-notes)
