# Login clean run — no debugger

- Capture date: 2026-08-16 TRT
- Accepted run tag: `20260816-142639-login-clean-retry`
- Purpose: answer only whether the corrected mode-2 launch reaches a visible
  username/password login window under a normal, elevated, debugger-free run.
- Verdict: **no login window appeared.** The client made its normal
  `ExitProcess(0)` request and the later crash/termination path produced the
  process status observed by `goley-boot`.

## Isolation and artifact identity

- `x32dbg.exe` was closed before launch and remained absent during the run.
- The wrapper stopped the `Windhawk` service and waited for `Stopped` before
  launching the target. Its original state was `Running` / `Automatic`; the
  wrapper restored it in `finally`, and a post-run check found it
  `Running` / `Automatic` again.
- Client: `C:\Joygame\Goley\BinaryTr\BinaryTr.bin`
- Client SHA-256:
  `C136E751905FBE60CF27A71D7D05FBDE7C0428484D577F3A0A56766C839EFCFA`
- `goley-boot.exe` SHA-256:
  `E8F7565839BB7D9272E185BC46F13E4F47C7D3A1AEFE3360FF05661026F7A121`
- `goley_shim.dll` SHA-256:
  `07FA308B65877AACAFF39A0A19502AB3F134B665F509B62340F6ED686DACC84A`
- Patch manifest SHA-256:
  `FE41E5C33FD1324DF9AF2A386622321FCBB8A195FA2775FD26FE7FCA195A884A`
- The shim's `patch_summary` reports build hash `C136...FCFA` and
  `applied: 0`. No client byte patch participated in this run.

## Exact command

The command was run elevated with working directory
`<ProjectRoot>`:

```powershell
& '<ProjectRoot>\target\i686-pc-windows-msvc\release\goley-boot.exe' run --client 'C:\Joygame\Goley\BinaryTr\BinaryTr.bin' --region TRAuth --shim '<ProjectRoot>\target\i686-pc-windows-msvc\release\goley_shim.dll' --patches '<ProjectRoot>\crates\goley-shim\patches\patches.toml' --timeout 60 -vv
```

The launch record identifies boot PID `22788` and client PID `25596`.
`goley-boot` itself returned `0`; its observation line records the client's
terminal status as decimal `1073741855`, i.e. `0x4000001F`.

## Window observation

The PID-scoped `EnumWindows` telemetry contains **70 samples** from
`221 ms` through `26,950 ms` after monitoring began (69 live samples and one
post-exit sample). Every sample has `Windows: []`: there was no target-owned
top-level HWND, and therefore no target window class or title, at any sampled
point. In particular, no username/password login window appeared.

The two desktop screenshots are retained outside the repository. Visual
inspection finds no Goley login UI in either image. The black/full-screen
surface visible in the captures belongs to another desktop application, not
to a `BinaryTr.bin` top-level window; the small nProtect updater UI is also
not the game login. Consequently the screenshots are environmental context,
not positive login evidence. The PID-scoped window telemetry is the decisive
observation.

## Termination sequence

The shim became ready at `2026-08-16T11:26:54.488602Z`. It then recorded:

1. At `2026-08-16T11:27:19.947799Z`, **25,459.197 ms after shim ready**,
   `BinaryTr.bin+0x4BA17C` called `ExitProcess` with status `0`.
2. At `2026-08-16T11:27:20.088294Z`, `BugTrap.dll+0x10DC0` called
   `TerminateProcess` with status `0xFFFFFFFF`.
3. At `2026-08-16T11:27:21.013170Z`, `ntdll.dll+0x6E1E0` called
   `NtTerminateProcess` with status `0x80000003`.

This is consistent with the already-established `+0x4BA17D` post-
`ExitProcess` INT3 aftermath: the target decision visible in this clean run
is the status-0 `ExitProcess` request, while `0x4000001F` is the final status
observed by the launcher after the shim suppressed noreturn termination and
BugTrap/NtTerminate ran. This clean run does not by itself identify which
downstream WinMain predicate chose the orderly-exit path.

## GameGuard preservation

- Pristine source:
  `%USERPROFILE%\Games\Goley\BinaryTr\GameGuard` — 32 files.
- Pre-run runtime backup:
  `%USERPROFILE%\Games\Goley-GameGuard-runtime-backup-pre-login-20260816-142639-login-clean-retry`
  — 26 files.
- Post-run runtime backup:
  `%USERPROFILE%\Games\Goley-GameGuard-runtime-backup-login-20260816-142639-login-clean-retry`
  — 26 files.
- Final canonical directory:
  `C:\Joygame\Goley\BinaryTr\GameGuard` — 32 files.

An independent recursive relative-path/length/SHA-256 manifest comparison
after the run found `32/32` files and zero differences between the pristine
source and the final canonical directory.

## Evidence artifacts

All runtime artifacts and screenshots remain outside the repository.

| Artifact | Absolute path | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| Status / exact command | `%USERPROFILE%\AppData\Local\Temp\goley-20260816-142639-login-clean-retry-status.json` | 2,393 | `70070C1D0C2212B571AC621A9690E5FD70DF76A8A9D8B395DE159C7C0780628D` |
| Raw shim JSONL | `%USERPROFILE%\AppData\Local\Temp\GoleyBoot-22788-18cc458ac25991c0-1-ready.jsonl` | 6,530 | `7C7E4A349D1A3D2178BE4D5A95AEAEBCE5357DCB2435B9FA48E29D99E8AAA900` |
| Window telemetry | `%USERPROFILE%\AppData\Local\Temp\goley-20260816-142639-login-clean-retry-telemetry.jsonl` | 6,904 | `1A0D81E50DF0DC4F4C9E575A1EE6BAF7D26FDD8FD09EC960A7E1D24CC9942BE5` |
| Boot stdout | `%USERPROFILE%\AppData\Local\Temp\goley-20260816-142639-login-clean-retry-boot-stdout.txt` | 392 | `1E1ACFBFC73E3DCD107AAC1527FE3C72D6A2AB8CE950751DEC5A1EE06DD5B227` |
| Boot stderr (empty) | `%USERPROFILE%\AppData\Local\Temp\goley-20260816-142639-login-clean-retry-boot-stderr.txt` | 0 | `E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855` |
| GameGuard restore record | `%USERPROFILE%\AppData\Local\Temp\goley-20260816-142639-login-clean-retry-gameguard.json` | 559 | `220B1D25F129213A39292ED158B43ECF9E72ED5919BF076752732F61943992BF` |
| Desktop screenshot, 3 s | `%USERPROFILE%\Downloads\goley-20260816-142639-login-clean-retry-3s.png` | 305,896 | `23634E30A0E77872CC44D039BE4C9C3177615C715D0CC0A4E937A37045750446` |
| Desktop screenshot, 10 s | `%USERPROFILE%\Downloads\goley-20260816-142639-login-clean-retry-10s.png` | 306,315 | `38835FA9145928211FE8A4A93CFBF5008CB84BAB391AE07DFFD45F51F5F61D36` |

## Discarded orchestration attempt

The earlier tag `20260816-142517-login-clean` is a **discarded orchestration
attempt**. Its target started, but at 3.337 seconds the PowerShell wrapper
threw while constructing the first screenshot because its `PixelFormat`
argument was passed as an invalid string conversion. The status record is
`%USERPROFILE%\AppData\Local\Temp\goley-20260816-142517-login-clean-status.json`;
the later recovery capture is
`%USERPROFILE%\Downloads\goley-20260816-142517-login-clean-recovered.png`.
They are retained only to diagnose the harness and are not used as target-
decision evidence. The retry documented above corrected only that screenshot
orchestration error and is the accepted clean run.

## Decision

With corrected unquoted `TRAuth NoPopup`, mode 2 still did **not** present a
login window in this clean, debugger-free run. The client remained alive for
about 26.95 seconds without creating a top-level window, requested
`ExitProcess(0)` at `BinaryTr.bin+0x4BA17C`, and then terminated through the
known BugTrap/NtTerminate aftermath. The next measurement therefore belongs
at the downstream WinMain gates; there is no evidence here for another
command-line change or a client-byte patch.
