# WinMain predicate — dynamic (hardware-breakpoint) confirmation

- Analysis date: 2026-08-16 TRT (~12:00–12:40 session clock)
- Method: deterministic `--pre-resume-gate` stop (primary thread still
  `CREATE_SUSPENDED`, code not yet unpacked) + elevated x32dbg attach +
  hardware execute breakpoints + direct PEB memory reads. **No byte was
  written to the client process, no register was written, and no source,
  `patches.toml`, or client byte was modified in this pass.** Only reads,
  breakpoint set/delete, and thread suspend/resume (debugger-side scheduling,
  not target data) were used.
- Related static evidence: `2026-08-16-exit-stack.md`,
  `2026-08-16-winmain-early-cleanup-cfg.md`,
  `2026-08-16-launcher-command-line.md`, `2026-08-16-clean-run.md`

## 0. Source-state correction (read before anything else)

The task brief's launcher-tail hypothesis — that `goley-boot` still quotes
`TRAuth`/`NoPopup` as `"...BinaryTr.bin" "TRAuth" "NoPopup"`, causing mode to
fall to `0` — describes an **earlier** state, not the code as it stands now.

`client_command_line` in `crates/goley-boot/src/windows_process.rs` (read,
not edited, this pass) already calls `push_unquoted_token` for both `TRAuth`
and `NoPopup`, and its own unit test asserts the corrected output:

```rust
assert_eq!(text, r#""C:\Joygame\Goley\BinaryTr\BinaryTr.bin" TRAuth NoPopup"#);
```

The currently built `target/i686-pc-windows-msvc/release/goley-boot.exe`
hashes to `E8F7565839BB7D9272E185BC46F13E4F47C7D3A1AEFE3360FF05661026F7A121`
— an **exact match** to the "unquoted" build already measured in
`2026-08-16-launcher-command-line.md` (the build that produced mode `2`
there). Nobody reverted this between that turn and this one. This pass
therefore measures the corrected source as-is, and — as shown below — Pass A
reconfirms mode `2`, not the mismatch the brief hypothesized.

## 1. Runtime image base / VA↔RVA

Confirmed identically across three independent live attaches (client PIDs
`23400`, `21656`, `21388`, all spawned by an elevated `goley-boot.exe
capture-waits --pre-resume-gate ...`):

| Field | Value |
| --- | --- |
| Runtime image base | `0x00400000` |
| Image size | `0x01598000` (22,642,688 bytes) |
| Reported PE entry (Themida stub, not real OEP) | `0x0197F000` |

This is an exact match to the base already assumed throughout
`2026-08-16-winmain-early-cleanup-cfg.md`. No RVA re-mapping is needed; every
VA quoted there and below is directly usable.

## 2. Pass A — launcher-tail: dynamically confirmed FALSE (fix already in effect)

Rather than racing a hardware breakpoint at `0x00d3faf3`/`0x00d3fb40` against
Themida's unpacking (see §4 for why that race is currently impractical), the
raw command line was read **directly from the live process's PEB** while the
primary thread was still parked at the pre-resume gate — i.e. before a single
instruction of client code had executed, so this is unconditionally what
`WinMainCRTStartup`/`GetCommandLineA` will see, not an inference:

1. `thread_list` → suspended thread's `teb = 0x00242000`.
2. Read `[teb+0x30]` → PEB = `0x0023F000`.
3. Read `[peb+0x10]` → `RTL_USER_PROCESS_PARAMETERS` = `0x01D973F8`.
4. Read `UNICODE_STRING CommandLine` at `[params+0x40]`: `Length=0x0076`,
   `MaximumLength=0x0078`, `Buffer=0x01D9791A`.
5. Read 118 bytes at the buffer (UTF-16LE), decoded to:

```text
"\\?\C:\Joygame\Goley\BinaryTr\BinaryTr.bin" TRAuth NoPopup
```

The byte immediately after the closing `"` of the path is `0x20` (space),
`0x00`; the next two bytes are `0x54`, `0x00` (`T`). There is **no `0x22`
(`"`) before `TRAuth` or `NoPopup`**. This is the direct opposite of the
brief's hypothesized tail (`"TRAuth" "NoPopup"`, first byte `0x22`).

**Conclusion:** with the command line WinMain actually receives, the `TR`
two-byte compare at `0x00d3fb38` will match, the write of mode `2` at
`0x00d3fb44` will execute, and `0x012bdc00` will hold `2`, not `0`. The
launcher-tail mismatch does **not** reproduce with the current,
unmodified `goley-boot.exe`. Pass A's hypothesis is dynamically refuted for
the present source state — this is a stronger result than the originally
planned breakpoint capture would have given, since it reads OS ground truth
(the PEB) with zero writes and zero race risk, rather than a register
snapshot at one instruction.

(`0x00d3fbbf`, Pass A's third checkpoint, is also one of the four hardware
breakpoints already armed from a prior session — see §4 for why it was not
reached live this pass.)

## 3. Infrastructure discovered/confirmed while attaching

- Elevation: `goley-boot.exe` carries a `requireAdministrator` manifest (per
  `docs/runtime/boot.md`); the sandboxed Bash/PowerShell shells in this
  session are **not** elevated (`IsInRole(Administrator) = False`, split
  token). `Start-Process -Verb RunAs` is required and triggers a real UAC
  consent prompt (`consent.exe`) that only an interactive human can approve;
  it cannot be scripted around, and `Stop-Process -Force` cannot dismiss it
  (protected process).
- `x32dbg` (the attached debugger, port `3000`, per `.mcp.json`) must
  **also** run elevated to attach to the elevated client — a non-elevated
  x32dbg's `attach` fails outright (`Direct command failed: attach .<pid>`)
  against a higher-integrity target. It was killed and relaunched elevated
  (PID `2412`) once this was diagnosed.
- The per-module breakpoint database
  `x64dbg\release\x32\db\BinaryTr.bin.dd32` already carried **4 enabled
  hardware execute breakpoints** from an earlier session, auto-reapplied on
  every fresh attach to a process named `binarytr.bin`, occupying all 4 x86
  debug-register slots:

  | VA | Meaning (from `2026-08-16-exit-stack.md` / `-winmain-early-cleanup-cfg.md`) |
  | --- | --- |
  | `0x008BD6ED` | instruction immediately after WinMain returns (`mov [ebp-0x20],eax`, saves the return value) |
  | `0x00D3FBBF` | first init-gate return, `test eax,eax` (Pass A priority 3) |
  | `0x00D3FC88` | second init-gate `test al,al` (Pass B) |
  | `0x00D40EAA` | the zero-return sink (`xor eax,eax`) |

  These are exactly the four highest-value CFG checkpoints, so they were left
  untouched rather than swapped for the raw Pass A byte-level addresses
  (already answered more directly via the PEB read in §2).
- `mcp__x64dbg__script_execute` / `script_execute_batch` are **blocked**
  server-side (`Method not allowed: script.execute`), so no native x32dbg
  command (batch continue, exception-ignore-list edit, etc.) could be issued
  — only the typed MCP verbs (`debug_run`, `breakpoint_set`,
  `debug_run_to`, register/memory reads, `thread_suspend`/`resume`) were
  available.

## 4. Why Pass B/C could not be reached live this pass

Releasing the pre-resume gate while attached puts execution through
Themida's SecureEngine-style unpacking stub, which dispatches many of its
own internal steps by deliberately executing a privileged instruction in
ring 3 (`sti`, opcode `FB`) to raise `STATUS_PRIVILEGED_INSTRUCTION`
(`0xC0000096`), catching it with its own handler as a control-flow/anti-
disassembly trick. `x32dbg.ini`'s `[Exceptions] IgnoreRange` is already
configured to pass every first-chance exception straight to the debuggee,
but each pass-through is still surfaced to the MCP caller as one
`debug_run` return (`stop_reason: breakpoint_or_exception`) rather than
being resolved in a loop server-side, and no batch/"skip N exceptions"
verb is exposed (see §3, `script_execute` blocked).

The sequence was confirmed **deterministic**: two independently gated runs
(PIDs `21656` and `21388`) reproduced an identical opening address sequence
(`0x018AD0C9 → 0x017B4295 → 0x017B4400 → 0x017B3C78 → 0x017B4E8E →
0x017B4B4A → 0x017F4011 → …`). After roughly 20 manual `debug_run`
continuations, execution reached genuine unpacked territory — confirmed by a
`Module loaded: winmm.dll` event and entry into `goley_shim.dll`'s own hook
code — but after **~95 total continuations** EIP was still deep inside the
main image, at addresses nowhere near the `0x00D3F000–0x00D41000` WinMain
region, still hitting `sti`-guarded dispatch cells (disassembly at several
stops showed the classic SecureEngine pseudo-random-constant pattern:
`push imm32; pop ecx; shl ecx,0x07; mov edx,imm32; …`), consistent with a
large VM-interpreted section-decryption loop rather than a short, bounded
anti-debug check. Full traversal to WinMain was not completed in this pass;
forcing it further by hand was judged disproportionate (per the task's own
"ZORLAMA" instruction) and was stopped rather than continued indefinitely.

A second, independent confound was identified and diagnosed along the way:
a `windhawk.dll` worker thread (third-party tool, already flagged as
"environmental noise" in `2026-08-16-clean-run.md`) got stuck re-faulting
`STATUS_ACCESS_VIOLATION` at an identical instruction
(`goley_shim.dll+0x7AE9`, later `+0x2E9CF`: `mov eax,[ecx+eax*4]` with
`ecx=0`, `eax=6` — a near-null dereference) inside the shim's own globally
installed API hooks (pattern consistent with a `tracing`
callsite/interest-cache lookup). Because the debugger halts every thread
whenever any one thread hits an event, and this thread re-faulted on
essentially every continue, it starved visibility into the client's real
primary-thread progress; `register_get_batch` confirmed bit-identical
`eax/ecx/esp/ebp` across repeated stops (genuinely stuck, not just
revisiting). `thread_suspend` on that thread id immediately restored forward
progress on the threads that matter; it was `thread_resume`d again before
ending the session. This is a real, reproducible shim/Windhawk interaction
worth a note for whoever next attaches through this phase — it is unrelated
to Goley's own WinMain logic.

`breakpoint_list` confirmed `hit_count: 0` on all four armed breakpoints for
the whole pass: none of Pass A's checkpoint‑3 (`0x00D3FBBF`), Pass B's
(`0x00D3FC88`), or the sink/WinMain-return breakpoints were reached live.

## 5. Mode-2 outcome: attempted, inconclusive this pass

Two attempts were made to at least answer "does mode `2` reach further than
mode `0` did" from process-exit telemetry, without needing to finish the
live CFG trace:

1. The gated/attached run (PID `21388`) was, after ~95 debug continuations,
   detached (via a disposable 32-bit dummy-process attach/kill, since no
   direct "detach" verb is exposed) to let it finish un-debugged. It had
   been held at breakpoints for roughly 10 minutes of real wall-clock time
   first — an abnormal dwell — and its shim log stopped after the
   `Global\MtxNPGL`/`Global\MtxNPGM` mutex sequence with **zero**
   `ExitProcess`/`TerminateProcess` events logged from inside the process,
   and no new WER Application Error 1000 record appeared. This differs from
   every mode-`0` baseline on record, but the result is **not trustworthy**:
   a 10-minute stall is exactly the kind of anomaly GameGuard's own
   watchdogs are designed to react to (e.g. an external `TerminateProcess`
   against the client, which the shim — hooking only calls made *from
   inside* the client process — would never see), so this cannot be
   attributed to the mode-2 fix itself.
2. Two follow-up **clean** (no gate, no debugger) `capture-waits` runs were
   started to get an uncontaminated answer. Both were killed by the
   environment before completing (the elevated launch left a `consent.exe`
   UAC prompt pending with no interactive approval arriving — the human
   operator was no longer at the keyboard to click through elevation partway
   through this pass). No approval was forced or faked; the orphaned prompts
   were left for the user and are otherwise harmless.

**Net: whether mode `2` alone reaches the login window, or still stalls at
the data-directory gate (`0x00d3fc88`) or one of the common initializer's
four false-return sources (`0x00d3f420`), remains unknown.** It is not
guessed here.

## 6. Net conclusion

- The task brief's premise — that `goley-boot` currently mis-quotes
  `TRAuth`/`NoPopup` — is **false for the code as it stands right now**.
  Nothing was changed to make this true; it was already fixed before this
  turn started (§0), and §2 dynamically reconfirms it from the live PEB.
- Because of that, mode **will** be written as `2` on the very next run,
  with the current, unmodified `goley-boot.exe`. Whatever prevents login
  today, if anything still does, is **not** the launcher-tail mismatch —
  it must be a different, downstream predicate. The two static candidates
  already on record (`2026-08-16-winmain-early-cleanup-cfg.md`) are the
  data-directory gate at `0x00d3fc88` and the common initializer's four
  false-return sources at `0x00d3f420`. This pass could not dynamically
  confirm or rule out either.
- No client byte, `patches.toml` entry, or `goley-boot`/shim source was
  modified in this pass.

## 7. Proposed fix (description only — nothing applied)

There is **no launcher-tail fix to apply** — it is already correct; do not
"fix" `client_command_line` again.

To make the remaining live-breakpoint work tractable next turn, one of:

- **(a) Bulk exception skip.** Enable `script_execute` just long enough to
  either (i) add `STATUS_PRIVILEGED_INSTRUCTION` (`0xC0000096`) to x32dbg's
  exception list as "ignore, don't stop the UI" rather than merely
  "pass to debuggee" (both are currently conflated by the MCP wrapper
  surfacing every passed exception), or (ii) issue x32dbg's native
  "run, skip N exceptions" command (`x32dbg.ini`'s existing
  `MaxSkipExceptionCount=0` suggests this is a recognized, just-disabled,
  knob) instead of one-at-a-time `debug_run`.
- **(b) A later deterministic gate.** Add a second, narrowly-scoped
  synchronization point to `goley-shim`, analogous to `--pre-resume-gate`
  but placed after Themida hands off to the real OEP and before the CRT
  calls WinMain (the shim already polls for exactly this transition for its
  own hook-install timing — see `docs/runtime/boot.md` "Themida readiness").
  Attaching there needs no exception-storm traversal at all. This is a real
  code change and is explicitly out of scope for a measurement-only pass.
- Whichever is chosen, keep the four already-armed breakpoints
  (`0x008BD6ED`, `0x00D3FBBF`, `0x00D3FC88`, `0x00D40EAA`) — they are
  correctly the highest-value checkpoints — and add Pass B's
  `0x00d35bf7` (candidate-path index) only after freeing a slot (delete
  `0x008BD6ED` temporarily, or wait for a WinMain-return hit before
  re-arming it).
- If a `windhawk.dll` thread ever re-faults at an identical EIP with
  identical registers across consecutive `debug_run` calls, `thread_suspend`
  that thread id (found via `thread_list`) before continuing further, and
  `thread_resume` it again before ending the session. This is debugger
  scheduling, not a target-process write.
- No byte patch is indicated by anything measured this pass; nothing in
  §2–§5 points at a specific RVA needing a `patches.toml` entry.

## 8. One-sentence next-turn summary

Confirm live whether mode `2` (already the default, dynamically verified via
direct PEB read — no code change needed) reaches the login window or still
stalls at the data-directory gate (`0x00d3fc88`) or the common initializer
(`0x00d3f420`), using a bulk exception-skip or a later post-unpack attach
point instead of single-stepping through Themida's dispatcher, and only once
interactive UAC approval is available again.
