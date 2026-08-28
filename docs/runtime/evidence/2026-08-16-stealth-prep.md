# Goley x32dbg stealth preparation evidence

## Scope

This step prepared the next controlled debugger comparison. It did not launch
the game client, install a kernel driver, add a breakpoint, or change workspace
source code.

## GameGuard preservation and pristine restore

Before restore, the canonical directory was the 26-file runtime state produced
by the clean run documented in `2026-08-16-clean-run.md`. No relevant game or
GameGuard process was alive.

That state was copied without transformation to:

```text
%USERPROFILE%\Games\Goley-GameGuard-runtime-backup-clean-run-20260816-010013
```

Its full SHA-256 manifest is:

```text
%USERPROFILE%\Games\Goley-GameGuard-runtime-backup-clean-run-20260816-010013.manifest.json
```

Source/backup verification: `26/26` files, `0` path/length/content-hash
differences.

The old canonical directory was then atomically moved, rather than recursively
deleted, to this additional staging copy:

```text
%USERPROFILE%\Games\Goley-GameGuard-runtime-staging-clean-run-20260816-010013
```

The pristine source was verified first:

```text
%USERPROFILE%\Games\Goley\BinaryTr\GameGuard
```

It contained 32 files and matched the clean-run pre-launch baseline 32/32 by
relative path, length, and SHA-256. The source verification manifest is:

```text
%USERPROFILE%\AppData\Local\Temp\goley-pristine-source-20260816-010013.json
```

It was copied to the canonical client location:

```text
C:\Joygame\Goley\BinaryTr\GameGuard
```

Post-restore verification: pristine source `32`, canonical destination `32`,
content differences `0`. Restore manifest:

```text
%USERPROFILE%\AppData\Local\Temp\goley-pristine-restore-20260816-010013.json
```

## ScyllaHide profile

The active elevated debugger executable was resolved as:

```text
<ToolsPath>\GoleyRE\x64dbg\release\x32\x32dbg.exe
```

Its ScyllaHide configuration is:

```text
<ToolsPath>\GoleyRE\x64dbg\release\x32\plugins\scylla_hide.ini
```

The original file was backed up before editing:

```text
<ToolsPath>\GoleyRE\x64dbg\release\x32\plugins\scylla_hide.ini.before-themida-20260816-010234.bak
```

| Item | Value |
| --- | --- |
| Original/backup SHA-256 | `B7EE4AB2F3B474B71981C7BA23CAAE417368E3B718895381AB39E58C07740D53` |
| Updated INI SHA-256 | `22F7A00FE72DED9A2FD02D3FE78A9F77637B23421DF158E6F412232DA6CAB69B` |
| Previous setting | `CurrentProfile=VMProtect x86/x64` |
| Verified setting | `CurrentProfile=Themida x86/x64` |
| Exact `[Themida x86/x64]` sections | `1` |

No profile body was invented or edited. `CurrentProfile` now selects the
already-installed official `[Themida x86/x64]` section. ScyllaHide's upstream
plugin loads `scylla_hide.ini` during `plugsetup`, so x32dbg was restarted after
the file update.

## TitanHide boundary

The x32 plugin file `TitanHide.dp32` exists, but the host has:

- `0` TitanHide service/system-driver records;
- no `HKLM\SYSTEM\CurrentControlSet\Services\TitanHide` key;
- no checked `TitanHide.sys` kernel-driver file.

TitanHide was not installed, registered, started, or otherwise changed.

## Controlled x32dbg restart

The one-use elevated wrapper was:

```text
%USERPROFILE%\AppData\Local\Temp\goley-x32dbg-restart-themida-20260816-010234.ps1
```

Wrapper SHA-256:
`6D5EB41F0DD08D6F36AF40A813FCFB81550BD81F8EF3612412D6566609AB6473`.

It requested a graceful `CloseMainWindow` for stopped x32dbg PID `16204`, made
no force-kill fallback, and started the same executable visibly with the
elevated token after UAC approval.

Result:

| Observation | Value |
| --- | --- |
| Wrapper interval | `2026-08-16 01:05:38.606 +03:00`–`01:05:40.205 +03:00` |
| Old PID graceful-close requested | `true` |
| Old PID closed | `true` |
| New x32dbg PID | `30508` |
| New x32dbg window | `x32dbg [Yükseltildi]` |
| `127.0.0.1:3000` listener owner | PID `30508` |
| x32dbg MCP state after restart | `stopped` |
| Profile after restart | `CurrentProfile=Themida x86/x64` |

Machine-readable restart result:

```text
%USERPROFILE%\AppData\Local\Temp\goley-x32dbg-restart-themida-20260816-010234.json
```

The debugger is now open, elevated, MCP-connected, has no debuggee, and is
ready for a controlled ScyllaHide/Themida-profile launch.
