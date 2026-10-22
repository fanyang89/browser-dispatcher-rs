"""Destructive registration smoke test for disposable GitHub-hosted runners only."""
import ctypes
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import time


def run(*args):
    subprocess.run([str(arg) for arg in args], check=True, timeout=120)


def windows_open(url):
    # Explicit class activation exercises the registered ProgID without changing
    # protected UserChoice or requiring Settings UI automation.
    from ctypes import wintypes as w

    class ExecuteInfo(ctypes.Structure):
        _fields_ = [
            ("cbSize", w.DWORD), ("fMask", w.ULONG), ("hwnd", w.HWND),
            ("lpVerb", w.LPCWSTR), ("lpFile", w.LPCWSTR),
            ("lpParameters", w.LPCWSTR), ("lpDirectory", w.LPCWSTR),
            ("nShow", ctypes.c_int), ("hInstApp", w.HINSTANCE),
            ("lpIDList", ctypes.c_void_p), ("lpClass", w.LPCWSTR),
            ("hkeyClass", w.HKEY), ("dwHotKey", w.DWORD),
            ("hIcon", w.HANDLE), ("hProcess", w.HANDLE),
        ]

    info = ExecuteInfo()
    info.cbSize = ctypes.sizeof(info)
    info.fMask = 0x00000001 | 0x00000100 | 0x00000400  # CLASSNAME, NOASYNC, NO_UI
    info.lpVerb = "open"
    info.lpFile = url
    info.lpClass = "BrowserDispatcherURL"
    info.nShow = 0
    shell = ctypes.WinDLL("shell32", use_last_error=True)
    shell.ShellExecuteExW.argtypes = [ctypes.POINTER(ExecuteInfo)]
    shell.ShellExecuteExW.restype = w.BOOL
    if not shell.ShellExecuteExW(ctypes.byref(info)):
        raise ctypes.WinError(ctypes.get_last_error())


def mac_defaults():
    cf = ctypes.CDLL("/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation")
    ls = ctypes.CDLL("/System/Library/Frameworks/CoreServices.framework/CoreServices")
    cf.CFStringCreateWithCString.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_uint]
    cf.CFStringCreateWithCString.restype = ctypes.c_void_p
    cf.CFRelease.argtypes = [ctypes.c_void_p]
    ls.LSCopyDefaultHandlerForURLScheme.argtypes = [ctypes.c_void_p]
    ls.LSCopyDefaultHandlerForURLScheme.restype = ctypes.c_void_p
    ls.LSSetDefaultHandlerForURLScheme.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    ls.LSSetDefaultHandlerForURLScheme.restype = ctypes.c_int
    saved = []
    for scheme in [b"http", b"https"]:
        key = cf.CFStringCreateWithCString(None, scheme, 0x08000100)
        handler = ls.LSCopyDefaultHandlerForURLScheme(key)
        if not handler:
            raise RuntimeError("No original default browser; refusing to modify associations")
        saved.append((key, handler))

    def restore():
        for key, handler in saved:
            status = ls.LSSetDefaultHandlerForURLScheme(key, handler)
            cf.CFRelease(handler)
            cf.CFRelease(key)
            if status:
                raise RuntimeError(f"Failed to restore default browser: {status}")
    return restore


def wait_records(directory, expected):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        actual = [path.read_text(encoding="utf-8").split("\0") for path in directory.glob("*.args")]
        if len(actual) == len(expected):
            assert sorted(actual) == sorted(expected), (actual, expected)
            return
        time.sleep(0.1)
    raise AssertionError(f"Timed out waiting for {len(expected)} browser invocations")


def main():
    if os.environ.get("GITHUB_ACTIONS") != "true" or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted":
        raise SystemExit("Only run on a disposable GitHub-hosted runner")
    windows = sys.platform == "win32"
    if not windows and sys.platform != "darwin":
        raise SystemExit("Windows/macOS only")
    suffix = ".exe" if windows else ""
    cli = Path("target/release/browser-dispatcher" + suffix).resolve()
    if windows:
        import winreg
        for key in [r"Software\Clients\StartMenuInternet\BrowserDispatcher", r"Software\Classes\BrowserDispatcherURL"]:
            try:
                existing = winreg.OpenKey(winreg.HKEY_CURRENT_USER, key)
            except FileNotFoundError:
                continue
            existing.Close()
            raise RuntimeError("Refusing to replace existing registration")
        stable = Path(os.environ["LOCALAPPDATA"]) / "browser-dispatcher"
        bundle = None
    else:
        stable = Path.home() / "Library/Application Support/browser-dispatcher"
        bundle = Path.home() / "Applications/BrowserDispatcher.app"
    assert not stable.exists(), "Refusing to replace existing install"
    assert bundle is None or not bundle.exists(), "Refusing to replace existing applet"
    restore = (lambda: None) if windows else mac_defaults()
    with tempfile.TemporaryDirectory(prefix="dispatcher smoke ") as temporary:
        work = Path(temporary)
        fake = work / ("fake browser" + suffix)
        records = work / "records"
        config = work / "custom config.toml"
        run("rustc", "tests/fixtures/browser.rs", "-o", fake)
        content = ('version = 1\ndefault_browser = "fake"\n[browsers.fake]\n'
                   f'path = {json.dumps(str(fake))}\nargs = ["work profile 中文"]\n')
        config.write_text(content, encoding="utf-8")
        expected = []
        try:
            # Reinstall checks idempotency and keeps the custom config binding.
            for _ in range(2):
                run(cli, "--config", config, "install", "--no-wizard")
                assert config.read_text(encoding="utf-8") == content
            sentinel = stable / "config.toml"
            sentinel.write_text("preserve this user configuration", encoding="utf-8")
            if windows:
                handler = stable / "browser-dispatcher-handler.exe"
                data = handler.read_bytes()
                pe = struct.unpack_from("<I", data, 0x3C)[0]
                assert data[pe:pe+4] == b"PE\0\0"
                assert struct.unpack_from("<H", data, pe + 24 + 68)[0] == 2, "Handler must use GUI subsystem"
            urls = [
                "https://example.com/?q=a&next=%22quoted%22#fragment",
                "https://example.com/%E4%B8%AD%E6%96%87?q=a'b&x=$HOME",
                "https://example.com/long?q=" + "x" * 4000,
            ]
            for url in urls:
                if windows:
                    windows_open(url)
                else:
                    run("/usr/bin/open", "-a", bundle, url)
                expected.append(["work profile 中文", url])
                wait_records(records, expected)
            # Config errors must not start any browser through the real handler.
            config.write_text("invalid TOML", encoding="utf-8")
            if windows:
                windows_open("https://example.com/invalid")
            else:
                # AppleScript may report a shell failure dialog, so config error
                # delivery is covered by the CLI integration suite on macOS.
                pass
            if windows:
                deadline = time.monotonic() + 30
                log = work / "dispatch.log"
                while "config_error=" not in log.read_text(encoding="utf-8"):
                    assert time.monotonic() < deadline
                    time.sleep(0.1)
                wait_records(records, expected)
        finally:
            try:
                restore()
            finally:
                run(cli, "uninstall")
        assert config.exists(), "Uninstall deleted custom config"
        assert sentinel.read_text(encoding="utf-8") == "preserve this user configuration"
        assert not (stable / ("browser-dispatcher" + suffix)).exists()
        if bundle:
            assert not bundle.exists()
        else:
            assert not (stable / "browser-dispatcher-handler.exe").exists()
            for key in [r"Software\Clients\StartMenuInternet\BrowserDispatcher", r"Software\Classes\BrowserDispatcherURL"]:
                try:
                    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, key):
                        raise AssertionError(f"Registration remains: {key}")
                except FileNotFoundError:
                    pass


if __name__ == "__main__":
    main()
