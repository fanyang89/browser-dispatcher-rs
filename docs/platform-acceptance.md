# Windows and macOS acceptance

## Automated coverage

`cargo test --all-targets --locked` exercises actual child-process launches using
`tests/fixtures/browser.rs`, not only rule previews. It covers normalized Unicode,
quotes, ampersands, fragments, long URLs, multiple URLs, private arguments, missing
executables, invalid executables, fallback, and invalid configuration. Windows
also tests a handler whose sibling CLI is missing.

The platform workflow runs `scripts/platform_smoke.py` on disposable GitHub-hosted
runners. This script modifies registration and therefore refuses local and
self-hosted execution. It refuses existing installations, installs twice with a
custom config, dispatches special URLs and a burst through native OS activation,
and verifies recorded browser arguments and uninstall preservation. macOS default
associations are saved and restored. Windows UserChoice is never overridden.

- Windows executes the installed ProgID command for HTTP(S) and separately uses
  normal ShellExecuteEx protocol activation through a temporary unique scheme
  bound to that same command. The temporary scheme is removed afterward. This
  avoids forced file-class activation, which can download a URL into INetCache
  rather than exercise protocol dispatch. The protected HTTP(S) default choice
  still requires desktop acceptance. The test checks the GUI PE subsystem, not
  the visual absence of all possible console flashes.
- macOS uses `open -a BrowserDispatcher.app`, delivering LaunchServices URL events.
- Neither explicitly targeted activation proves that a user's default-browser
  choice works. Neither a fake browser nor a successful process spawn proves a
  real browser displayed the requested page.
- Reinstalling the same binary tests idempotency, not a version-to-version upgrade.
- The macOS malformed-config AppleScript dialog remains a manual check to avoid
  hanging headless runners on a modal alert.

## Required desktop acceptance (not yet verified)

Use a disposable user account or VM. Record OS version/architecture, application
version, browser version, result, and screenshots or relevant logs. Do not use
sensitive URLs: dispatch logs currently include full URLs.

1. Install a release archive, including both executables on Windows. Register
   using a custom config path containing spaces and non-ASCII characters.
2. Select Browser Dispatcher as the default for HTTP and HTTPS in system settings.
   Click both types of links from another desktop application, without explicitly
   selecting Browser Dispatcher. Verify the rule-selected browser and URL.
3. Test Chrome, Edge and Firefox on Windows; Safari, Chrome and Firefox on macOS.
   For each, test a fully closed browser and an already-running browser. Confirm
   the page opens, focus behavior is sensible, and existing windows are reused
   where expected. macOS still uses browser executables directly; compatibility
   must be established per browser before changing that launch strategy.
4. Test private rules and Firefox profiles. Unsupported private mode must fail,
   never silently open a normal window. Repeat with a missing target and a valid
   fallback, then with both targets unavailable.
5. Click several links rapidly and verify no URLs are lost, duplicated, or changed.
   Include encoded Unicode, quotes, `&`, fragments and a long query string.
6. On Windows, watch for console flashes during both successful and failed URL
   activation. Remove the sibling CLI in a disposable copy and confirm failure
   is recorded in `handler.log` alongside the handler.
7. Corrupt the configuration, click a link, and verify no browser opens. Inspect
   `dispatch.log` and any macOS AppleScript error dialog, then repair the config.
8. Upgrade from an older release, reinstall, and repeat URL activation. Verify
   that registration uses the new binary and the intended custom config.
9. Select a replacement default browser, uninstall, verify registration removal
   and preservation of custom config and colocated config/log files, then reinstall.

`started` in dispatch.log means process creation succeeded, not page load success.
