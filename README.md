# Browser Dispatcher

A small Rust CLI that can become the system default browser and route each HTTP/HTTPS URL to Firefox, Chrome, Edge, or another browser according to a TOML configuration file.

Supported registration targets:

- Windows (per-user registry registration)
- macOS (LaunchServices + a lightweight AppleScript URL-handler applet)
- Linux/XDG (extra, useful for development and desktop Linux)

## Features

- `clap`-based CLI
- First-match-wins host glob and full-URL regex rules
- Built-in browser detection for Firefox, Chrome, Chromium, Edge, Brave, Vivaldi, Opera, and Safari (macOS)
- Custom browser executable paths and arguments
- Private/incognito mode per rule
- Interactive installation wizard
- `doctor` validation for config, browser executables, OS registration, installed binary freshness, and an end-to-end dry-run
- Dispatch log with simple size rotation
- GoReleaser archives and checksums for Linux, Windows, and macOS

## Quick start

```sh
cargo install --path .
browser-dispatcher install
browser-dispatcher doctor
```

`install` is idempotent. Run it again after upgrading or moving the CLI.

Preview routing without opening a browser:

```sh
browser-dispatcher dispatch --dry-run https://login.microsoftonline.com/
```

The OS invokes the installed handler as:

```sh
browser-dispatcher https://example.com/
```

A bare URL argument and the explicit `dispatch` subcommand behave the same way.

## Commands

```text
browser-dispatcher install [--no-wizard] [--default-browser <id>]
browser-dispatcher doctor
browser-dispatcher dispatch [--dry-run] <url>...
browser-dispatcher uninstall
```

Use `--config <path>` globally to override the default config location.

### `install`

1. Copies the current binary to a stable per-user location.
2. Creates a commented default TOML config if none exists.
3. Registers Browser Dispatcher as an HTTP/HTTPS handler.
4. Runs an interactive rule wizard unless `--no-wizard` is used or no TTY is available.

Platform details:

- **Windows:** writes per-user entries under `HKCU\\Software\\Clients\\StartMenuInternet`, `HKCU\\Software\\RegisteredApplications`, and `HKCU\\Software\\Classes`. URL associations invoke the GUI-subsystem `browser-dispatcher-handler.exe`, which forwards to the CLI without opening a console window. On current Windows 11 builds, `install` opens the app-specific `ms-settings:defaultapps?registeredAppUser=Browser%20Dispatcher` page. Windows protects the active `UserChoice` association with a hash, so the user must click **Set default**.
- **macOS:** copies the Rust CLI to `~/Library/Application Support/browser-dispatcher/`, creates `~/Applications/BrowserDispatcher.app` with `osacompile`, registers its `GURL` handler with LaunchServices, and requests it as the default for `http`/`https`. If macOS declines the automatic change, choose Browser Dispatcher in **System Settings > Desktop & Dock > Default web browser**.
- **Linux:** creates `~/.local/share/applications/browser-dispatcher.desktop` and calls `xdg-settings` when available.

### `doctor`

`doctor` checks:

- config file existence, TOML syntax, and schema version
- rule matchers and referenced browser ids
- browser executable discovery
- OS handler registration and active default associations
- installed binary freshness
- execution of the installed binary via `dispatch --dry-run`
- representative rule decisions

It exits non-zero when a required check fails.

### `uninstall`

Removes the browser registration and installed binary/app bundle. The config file is deliberately kept. Windows/macOS users must pick a replacement default browser in system settings.

## Configuration

Default locations:

| OS | Path |
| --- | --- |
| Windows | `%APPDATA%\\browser-dispatcher\\config.toml` |
| macOS | `~/Library/Application Support/browser-dispatcher/config.toml` |
| Linux | `$XDG_CONFIG_HOME/browser-dispatcher/config.toml` (usually `~/.config/...`) |

Example:

```toml
version = 1
default_browser = "firefox"

[[rule]]
name = "microsoft-login"
host = ["login.microsoftonline.com", "*.microsoft.com"]
browser = "edge"

[[rule]]
name = "local-development"
url_regex = '^https?://([a-z0-9-]+\.)?localhost(:\d+)?/'
browser = "chrome"
private = true

[browsers.firefox]
# path = "C:/Program Files/Mozilla Firefox/firefox.exe"
profile = "default-release"
args = []

[browsers.librewolf]
path = "/Applications/LibreWolf.app/Contents/MacOS/librewolf"
private_args = ["-private-window"]
```

See [`config.example.toml`](config.example.toml) for a fuller example.

### Rule semantics

- Rules are evaluated from top to bottom; the first match wins.
- `host` accepts a string or an array of strings.
- Host matching is case-insensitive.
- `*.example.com` matches `example.com`, `a.example.com`, and nested subdomains.
- `url_regex` matches the normalized full URL and is case-sensitive.
- A rule matches when either its `host` or `url_regex` matcher succeeds.
- Bare inputs such as `example.com` are normalized to `https://example.com/`.
- `private = true` uses the browser's known private/incognito flag. Custom browsers can define `private_args`.
- If a matched browser cannot launch, the dispatcher tries `default_browser` once.

### Browser overrides

Every browser entry supports:

| Field | Meaning |
| --- | --- |
| `path` | Explicit executable path; overrides auto-detection |
| `args` | Extra arguments passed before the URL |
| `profile` | Firefox profile name (`-P <name>`) |
| `private_args` | Private-window flags for a custom browser |

## Logs

Dispatch results are appended next to the config file as `dispatch.log`. It rotates automatically after 256 KiB. `doctor` prints the five most recent entries.

## Releases

Tags matching `v*` trigger [`.github/workflows/release.yml`](.github/workflows/release.yml). The Cargo package version should match the tag, for example `version = "0.2.0"` with tag `v0.2.0`.

Published artifacts:

- Linux x86_64 musl: `.tar.gz`
- Windows x86_64 GNU: `.zip` (CLI + console-free URL handler)
- macOS x86_64 and arm64: `.tar.gz`
- SHA-256 checksums: `checksums.txt`

The release workflow runs on macOS so native Apple frameworks are available. It uses native Cargo for macOS targets and `cargo-zigbuild` for Linux/Windows targets. A manually dispatched workflow produces a non-published snapshot by default.

Validate the configuration locally:

```sh
goreleaser check
```

A full local snapshot requires macOS, Zig, and `cargo-zigbuild`:

```sh
goreleaser release --snapshot --clean
```

## Development

```sh
cargo fmt -- --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Cross-platform type-checks from a machine with the targets installed:

```sh
cargo check --target x86_64-pc-windows-msvc
cargo check --target aarch64-apple-darwin
```

For distributable macOS builds, replace ad-hoc signing with a Developer ID signature and notarization. The local `install` workflow uses ad-hoc signing for a user-built applet.

## License

MIT
