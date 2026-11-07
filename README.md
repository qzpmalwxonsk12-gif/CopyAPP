# CopyAPP

CopyAPP is a private, local-first clipboard manager for Windows. It runs in the
system tray, records supported clipboard formats in SQLite, and lets you paste
one of the ten newest entries without opening the window.

## Features

- Captures plain text, HTML/rich text, screenshots/images, and copied files.
- Keeps a persistent chronological history without moving old entries when
  they are reused.
- Maps the newest ten entries to `Ctrl+V+1` through `Ctrl+V+9`, with
  `Ctrl+V+0` selecting the tenth entry. Keep Ctrl held between V and the digit.
- Keeps normal `Ctrl+V` available when no slot digit follows.
- Provides search, type filters, pins, previews, copy/paste, individual delete,
  and pinned-safe history clearing.
- Supports customizable slot/window shortcuts, recording pause, application
  exclusions, light/dark themes, and optional Windows startup.
- Has no account, network service, telemetry, analytics, or cloud sync.

Clipboard contents and settings are stored only in the current user's Windows
LocalAppData directory. The exact database path is shown on the Settings page.

## Development

Prerequisites:

- Windows 10 or 11 with the WebView2 runtime
- Node.js 20.19+ or 22.12+
- Rust stable with the `x86_64-pc-windows-msvc` toolchain
- Visual Studio C++ Build Tools and a Windows 10/11 SDK

```powershell
npm.cmd install
npm.cmd run tauri:dev
```

Quality checks and a release build:

```powershell
npm.cmd run check
npm.cmd run build
C:\Users\<you>\.cargo\bin\cargo.exe test --manifest-path src-tauri\Cargo.toml
npm.cmd run tauri:build
```

Release installers are written below `src-tauri\target\release\bundle`.

## Architecture

- `src/` contains the React/TypeScript interface and typed Tauri adapter.
- `src-tauri/src/clipboard.rs` owns multi-format clipboard capture/restore.
- `src-tauri/src/keyboard.rs` owns the Windows low-level keyboard hook and input
  injection. Hook callbacks enqueue work; they never touch SQLite or clipboard
  payloads directly.
- `src-tauri/src/db.rs` owns the SQLite schema and ordered history queries.
- `src-tauri/src/platform.rs` contains foreground-app, exclusions, focus, and
  Windows startup integration.

The clipboard watcher and shortcut worker run outside the UI thread. SQLite
uses WAL mode, normal synchronization, and a busy timeout. Consecutive semantic
duplicates are ignored, and application-initiated clipboard writes are
suppressed so using an old entry never creates or reorders history.

## Shortcut note

The numbered paste sequence cannot be represented by a conventional global
hotkey because it contains two non-modifier keys. CopyAPP therefore uses
`WH_KEYBOARD_LL` on Windows, preserves the physical Ctrl state, ignores its own
injected input, and forwards an ordinary paste when the sequence is not
completed.
