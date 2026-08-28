# GameGuard Error99(0): measured periodic-status failure

- Capture date: 2026-08-16 TRT
- Client: `C:\Joygame\Goley\BinaryTr\BinaryTr.bin`
- Client SHA-256:
  `C136E751905FBE60CF27A71D7D05FBDE7C0428484D577F3A0A56766C839EFCFA`
- Image base: `0x00400000`
- Verdict: the first failing periodic GameGuard status check returned
  **`610` (`0x00000262`)**.
- Client-side error path: measured at RVA `0x0093BB73` / VA `0x00D3BB73`.
- Verification status: **PASS**. Clean run
  `20260816-170733-error99-patched` kept the login client alive for the full
  150-second observation window without a `GameGuard Error` dialog.

This follows the login milestone in
`docs/runtime/evidence/2026-08-16-login-reached.md`. The existing
RVA-`0x009374DB` patch accepts updater-unavailable initialization status 380;
it does not affect the later periodic status function documented here.

## Dynamic measurements

Both probes used the shim's existing byte-free x86 DR0 execute gate. The gate
validated `STATUS_SINGLE_STEP` (`0x80000004`), exception address, EIP, DR0 and
DR6.B0 before redirecting normal flow to its register-preserving wait thunk.
No software breakpoint or client-file write was used.

### Control: first periodic check succeeds

The first probe stopped immediately after the periodic status call, at the
comparison instruction:

| Field | Value |
| --- | --- |
| Run tag | `20260816-170148-error99-eax` |
| Gate RVA / VA | `0x0093BB6C` / `0x00D3BB6C` |
| EAX | `1877` / `0x00000755` |
| EIP | `0x00D3BB6C` |
| DR0 | `0x00D3BB6C` |
| DR6.B0 | set |

This is a positive control: the first periodic check takes the client's
success branch and therefore cannot identify the later popup status by
itself.

### Failure-only checkpoint

The second probe moved the execute gate to the first instruction of the
failure block. That address is reachable only after the comparison against
`0x755` fails. Because an execute breakpoint is delivered before the target
instruction executes, EAX still contains the return value from the periodic
GameGuard status call.

| Field | Value |
| --- | --- |
| Run tag | `20260816-170311-error99-fail` |
| Gate RVA / VA | `0x0093BB73` / `0x00D3BB73` |
| EAX | **`610` / `0x00000262`** |
| EBX | `0` |
| EIP | `0x00D3BB73` |
| EFLAGS | `0x00000293` |
| DR0 | `0x00D3BB73` |
| DR6 | `0xFFFF0FF1` (B0 set) |
| DR7 | `0x00000001` |
| Primary TID | `27068` |

The metadata independently records the validated exception address and EIP
as `0x00D3BB73`, an execute-breakpoint length of one, original DR0--DR7 all
zero, armed DR0 `0x00D3BB73`, and armed DR7 `0x00000001`.

The exact measured command was:

```powershell
& '<ProjectRoot>\target\i686-pc-windows-msvc\release\goley-boot.exe' run `
  --client 'C:\Joygame\Goley\BinaryTr\BinaryTr.bin' `
  --region TRAuth `
  --runparam-key NMRP20260816LOCALKEY0001 `
  --oep-rva 0x009374DB `
  --late-inject-ms 3000 `
  --shim '<ProjectRoot>\target\i686-pc-windows-msvc\release\goley_shim.dll' `
  --patches '<ProjectRoot>\crates\goley-shim\patches\patches.toml' `
  --post-unpack-gate '%USERPROFILE%\AppData\Local\Temp\goley-20260816-170311-error99-fail.release' `
  --post-unpack-gate-rva 0x0093BB73 `
  --post-unpack-gate-timeout 120 `
  --timeout 150 -vv
```

It inherited the same valid `NMRunEnv_*` launcher envelope recorded in the
login evidence. The raw shim trace confirms that only the already-measured
RVA-`0x009374DB` initialization patch was active during this measurement
(`patch_summary.applied = 1`).

## Static production path

The static chain was decoded read-only from the previously captured flat
mapped image
`%USERPROFILE%\Downloads\goley-unpacked-20260816-011037.dump` (22,642,688
bytes, SHA-256
`AA34F37364069652ED7CE6AAB105DD43BA7F61D7C09EC2A231B1B9E9D44D7BA1`).

### Call chain

| Stage | RVA / VA | Observation |
| --- | --- | --- |
| Frame/tick caller | `0x00940E82` / `0x00D40E82` | Calls the update function at `0x00D3B550`. |
| Periodic consumer | `0x0093BB67` / `0x00D3BB67` | Calls shared status wrapper `0x008E4000`. |
| Shared wrapper | `0x004E4000` / `0x008E4000` | Returns zero if manager pointer `[0x012B4288]` is null; otherwise tail-jumps to `0x008E6C40`. |
| Status implementation | `0x004E6C40` / `0x008E6C40` | Implements the readiness/grace counter and returns `0x755` or a specific failure value. |
| Compare | `0x0093BB6C` / `0x00D3BB6C` | `cmp eax, 0x755`. |
| Failure edge | `0x0093BB73` / `0x00D3BB73` | First instruction after the success-only conditional jump. The measured EAX here was `0x262`. |
| Error formatting | `0x0093BB90..0x0093BBA1` | Formats `Gameguard Error99(%lu)` into global buffer `0x012BC9C8`. |
| Dialog consumer | `0x0093B7CB..0x0093B7F2` | On the following tick, presents the nonempty buffer under title `GameGuard Error`. |

The decisive instruction window is:

```text
00D3BB67  E8 94 84 BA FF       call 0x008E4000
00D3BB6C  3D 55 07 00 00       cmp  eax,0x755
00D3BB71  74 31                je   0x00D3BBA4
00D3BB73  B8 C8 C9 2B 01       mov  eax,0x012BC9C8
```

The exact string addresses and direct references are:

| Data | RVA / VA | Direct reference |
| --- | --- | --- |
| `Gameguard Error99(%lu)` | `0x00B86E14` / `0x00F86E14` | `0x00D3BB92` |
| `GameGuard Error` | `0x00B86E2C` / `0x00F86E2C` | `0x00D3B7E7` |

The dialog observed in the prior telemetry belonged to the client PID, so
this path is client UI code rather than a `ggerror.des` child window.

### What `(0)` means

The displayed suffix is not the failing status. The update function clears
EBX at `0x00D3B593`; the error formatter later executes `push ebx` at
`0x00D3BB90`. Consequently the format argument is always zero, producing
`Gameguard Error99(0)` regardless of the EAX status returned by
`0x008E4000`. The failure-only DR0 snapshot is the measurement that establishes
the actual status as `0x262`.

### Meaning of `0x262`

The periodic implementation increments counter `[0x012B428C]`. When manager
byte zero remains clear, it temporarily returns success for the grace checks;
once the incremented counter reaches three, the path at
`0x008E6C90..0x008E6CC4` returns `0x262`. The caller evaluates this path every
five seconds; the comparison constant at VA `0x00FB3530` is the IEEE-754
double value `5.0`. Thus the measured error is the manager-not-active result
after its grace polls, not initialization status 380 and not numeric status
zero.

## Persisted compatibility patch

The narrowest measured consumer-only change has been added to
`<ProjectRoot>\crates\goley-shim\patches\patches.toml`:

| Field | Value |
| --- | --- |
| RVA / VA | `0x0093BB67` / `0x00D3BB67` |
| Original bytes | `E8 94 84 BA FF` (`call 0x008E4000`) |
| Patched bytes | `B8 55 07 00 00` (`mov eax, 0x755`) |
| Build guard | `C136E751905FBE60CF27A71D7D05FBDE7C0428484D577F3A0A56766C839EFCFA` |

This preserves the client's existing comparison and success edge while
removing only the periodic poll used by the Error99 consumer. It deliberately
does not replace or hook shared function `0x008E4000`, whose other direct
caller at `0x00D37870` remains unchanged. The original five bytes were
verified in the mapped image before the record was persisted.

At the time of verification, the manifest contained both measured records:
the initialization-status-380 compatibility gate and this periodic-status
consumer gate. Its SHA-256 was
`01CA8939FDE521A1D3555D60BC42D4859D4C0D9EF6447B7DFBE75C05CBE7F0FB`.

## Clean verification: PASS

Run `20260816-170733-error99-patched` used the valid launcher envelope with no
debugger and no post-unpack gate. The exact command was:

```powershell
& '<ProjectRoot>\target\i686-pc-windows-msvc\release\goley-boot.exe' run `
  --client 'C:\Joygame\Goley\BinaryTr\BinaryTr.bin' `
  --region TRAuth `
  --runparam-key NMRP20260816LOCALKEY0001 `
  --oep-rva 0x009374DB `
  --late-inject-ms 3000 `
  --shim '<ProjectRoot>\target\i686-pc-windows-msvc\release\goley_shim.dll' `
  --patches '<ProjectRoot>\crates\goley-shim\patches\patches.toml' `
  --timeout 150 -vv
```

The shim trace records two build-keyed `static_patch` events, at RVAs
`0x009374DB` and `0x0093BB67`, followed by `patch_summary.applied = 2`. The
bounded run then produced 355 telemetry samples: zero contained
`GameGuard Error`, 295 contained the client window class `ChaguClass`, and
none recorded `Alive:false`. The client therefore remained alive for the full
150-second window. The screenshot captured at 75 seconds shows the Turkish
username/password login panel without the former popup:

`%USERPROFILE%\Downloads\goley-20260816-170733-error99-patched-75s.png`

The screenshot SHA-256 is
`C4E9DAB04B3C0233CBC0024409F56115C6A6D69AE3D03E17DB003725CF94F7AB`.
This clean run passes the former approximately 56-second Error99 boundary and
accepts the consumer-only patch as runtime verified.

## Artifact identities and preservation

| Artifact | Absolute path | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| First-success status | `%USERPROFILE%\AppData\Local\Temp\goley-20260816-170148-error99-eax-status.json` | 5,908 | `6431F396E1DE41F2A501F16685E7BF4D9D594B39DBB5B97FC3E23182BA37360B` |
| First-success raw shim trace | `%USERPROFILE%\AppData\Local\Temp\GoleyBoot-23060-18cc4dff6045c7d8-1-ready.jsonl` | 12,842 | `375180503AA2CB70F68D6743A9C9A932C4A4282936133BC6638DE68DE48B76BF` |
| First-success gate metadata | `%USERPROFILE%\AppData\Local\Temp\goley-20260816-170148-error99-eax.release.metadata.json` | 1,554 | `D8702D25CB14A7F3146514C003049106518478D4020F4D603134CE85A94953C3` |
| Failure status and command | `%USERPROFILE%\AppData\Local\Temp\goley-20260816-170311-error99-fail-status.json` | 5,912 | `D8933E199001AF079D098104C8CE77121B70F19CBB845A8DAA6B6523C9DFF405` |
| Failure raw shim trace | `%USERPROFILE%\AppData\Local\Temp\GoleyBoot-26988-18cc4e12b542eb7c-1-ready.jsonl` | 12,841 | `B4CFD023CB7603FB8A8D1E2009E8CA544C4A5EB5DAE3EA3F1FBD4A8B9D24AB88` |
| Failure gate metadata | `%USERPROFILE%\AppData\Local\Temp\goley-20260816-170311-error99-fail.release.metadata.json` | 1,555 | `E455DD1FB824A46EC9259C6F4D048A9DEA51200B2B878041E0CDB280E84BD2E2` |
| Failure launcher stdout | `%USERPROFILE%\AppData\Local\Temp\goley-20260816-170311-error99-fail-boot-stdout.txt` | 802 | `0666006D77A16938D2F6EEE6FAA4ECF151C83FFAA64E43DF820EE79D199B168C` |
| Clean-verification status and command | `%USERPROFILE%\AppData\Local\Temp\goley-20260816-170733-error99-patched-status.json` | 8,433 | `1320A368AB03779B641A0336E94715F6E4D9EB6C2A53B6DD84BF449EC3367A97` |
| Clean-verification raw shim trace | `%USERPROFILE%\AppData\Local\Temp\GoleyBoot-24432-18cc4e4fe9ee7084-1-ready.jsonl` | 12,272 | `8D6688155275C1C06CB58952834AA075AD0A2A7A39183FF7E59B31F34F877C11` |
| Clean-verification telemetry | `%USERPROFILE%\AppData\Local\Temp\goley-20260816-170733-error99-patched-telemetry.jsonl` | 174,104 | `B2661377D75816D628CD43B0F223D0E88AA106846779D43F5EFE49BB01559A48` |
| Popup-free login screenshot at 75 seconds | `%USERPROFILE%\Downloads\goley-20260816-170733-error99-patched-75s.png` | 2,374,610 | `C4E9DAB04B3C0233CBC0024409F56115C6A6D69AE3D03E17DB003725CF94F7AB` |

The failure measurement backed up the 26-file post-run GameGuard tree to
`%USERPROFILE%\Games\Goley-GameGuard-runtime-backup-post-20260816-170311-error99-fail`.
It then restored the canonical GameGuard directory from the pristine source;
the final record contains 32 files and `FinalRestoreDiffCount = 0`. The
first-success control independently records the same 32-file, zero-difference
final restore result.

The clean verification backed up the 26-file post-run GameGuard tree to
`%USERPROFILE%\Games\Goley-GameGuard-runtime-backup-login-20260816-170733-error99-patched`.
It then restored the canonical directory to 32 files; the status record's
`FinalRestoreDiff` object is empty, so the final comparison is 32/32 with zero
differences.

## Decision

The Error99 phase is complete. Runtime measurement established the actual
periodic failure value as `0x262`, static analysis tied that value to the
unique Error99 consumer, the manifest applied the original-byte-guarded
consumer-only change, and the clean run verified a stable popup-free login
screen for 150 seconds. Redirecting the client's entry-server connection and
measuring the ProudNet handshake are separate follow-on work.
