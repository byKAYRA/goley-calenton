# goley-boot

`goley-boot` is the Windows launcher/orchestrator for the clean-room Goley
runtime work. It starts the unmodified client suspended, injects the companion
`goley-shim.dll`, performs a two-stage named-event handshake, then resumes and
observes the client.

The production target is **`i686-pc-windows-msvc`**. Both the launcher and the
shim must be 32-bit because the fixed client is a PE32/x86 process.

```powershell
rustup target add i686-pc-windows-msvc
cargo build -p goley-boot -p goley-shim --target i686-pc-windows-msvc

Start-Process -Verb RunAs `
  -FilePath target\i686-pc-windows-msvc\debug\goley-boot.exe `
  -WorkingDirectory $PWD `
  -ArgumentList @(
    "run", "--client", "C:\Joygame\Goley\BinaryTr\BinaryTr.bin",
    "--region", "TRAuth"
  )

Start-Process -Verb RunAs `
  -FilePath target\i686-pc-windows-msvc\debug\goley-boot.exe `
  -WorkingDirectory $PWD `
  -ArgumentList @(
    "capture-waits", "--client", "C:\Joygame\Goley\BinaryTr\BinaryTr.bin",
    "--timeout", "30"
  )

# The output parent must already exist and must be outside this repository.
Start-Process -Verb RunAs `
  -FilePath target\i686-pc-windows-msvc\debug\goley-boot.exe `
  -WorkingDirectory $PWD `
  -ArgumentList @(
    "dump-unpacked", "--client", "C:\Joygame\Goley\BinaryTr\BinaryTr.bin",
    "--out", "$env:TEMP\goley-unpacked.dump"
  )
```

The executable embeds a `requireAdministrator` manifest because the fixed
client also requests elevation. `Start-Process -Verb RunAs` makes the UAC step
explicit when starting from a non-elevated terminal. A rejected/missing
elevation is reported as `ERROR_ELEVATION_REQUIRED (740)`.

`--shim` can select a DLL explicitly. Otherwise resolution checks
`GOLEY_SHIM_DLL`, the launcher's directory, and conventional sibling target
directories. Configuration is passed through a Unicode environment block; no
client bytes are modified by this crate.

After the shim signals `READY`, `run` deliberately retains the process handle
and observes the client until it exits. Pass `--detach` to return immediately
after `READY` while leaving the client running. The default post-readiness
settling value is 8 ms and is delivered to the shim as data; the launcher does
not implement hook timing with a blind sleep.

For a deterministic x32dbg attach, `run` and `capture-waits` accept
`--pre-resume-gate PATH` and `--pre-resume-gate-timeout SECONDS`. After the
shim signals `LOADED`, the launcher keeps the client primary thread suspended,
writes the child PID and measured main-image base to both stdout and
`PATH.metadata.json`, then waits for `PATH` to be created as a regular file.
This permits deleting automatic software breakpoints and installing a hardware
execute breakpoint before release. Gate and metadata paths must be fresh; a
stale file is rejected rather than consumed. Example release command:

```powershell
New-Item -ItemType File -Path "$env:TEMP\goley.resume"
```

Omitting `--pre-resume-gate` preserves the ordinary launch sequence. The gate
is intentionally absent from `dump-unpacked`, whose pre-resume baseline timing
remains unchanged.

`run` and `capture-waits` also accept a byte-free gate at a measured
main-image RVA:

```powershell
& target\i686-pc-windows-msvc\release\goley-boot.exe run `
  --client C:\Joygame\Goley\BinaryTr\BinaryTr.bin --region TRAuth `
  --shim target\i686-pc-windows-msvc\release\goley_shim.dll `
  --patches crates\goley-shim\patches\patches.toml `
  --post-unpack-gate "$env:TEMP\goley-post-unpack.release" `
  --post-unpack-gate-rva MEASURED_RVA `
  --post-unpack-gate-timeout 120
```

Both `--post-unpack-gate` and `--post-unpack-gate-rva` are required together.
Before the suspended primary thread resumes, the launcher range-checks
`image_base + RVA`, rejects an occupied DR0 slot, arms an x86 execute
breakpoint, and verifies DR0/DR6/DR7. The shim installs its first-priority VEH
before `LOADED`. On the exact `STATUS_SINGLE_STEP`/address/EIP/DR6.B0/DR0 hit,
the VEH clears its slot and redirects normal flow to a GPR/EFLAGS/FPU/SSE
preserving thunk. The thunk signals ARRIVED and waits through the raw ntdll
API, after exception dispatch has finished; an attached debugger can therefore
reuse all four hardware-breakpoint slots without a later `NtContinue` erasing
them.

Only after ARRIVED does the launcher atomically publish
`PATH.metadata.json`. It contains PID, primary TID, image extent, target
RVA/VA, original and armed debug-register values, event names, and the two
deadlines. Create the fresh `PATH` file to release the thunk. A release marker
created before metadata is a protocol error. Launcher errors signal the release
event before terminating the observed child, and the shim has a finite
fail-open deadline 10 seconds beyond the launcher deadline. Omitting these
options is the existing default behavior; `dump-unpacked` does not expose this
gate.

`dump-unpacked` does not use a built-in OEP or a blind delay. Immediately after
the shim's `LOADED` handshake—and while the client's primary thread is still
suspended—it records the complete executable-page state of the main image.
After resume it requires both a real deviation from that baseline and an
uninterrupted executable-page quiescence window (100 ms by default). A full
image read is bracketed by two matching executable-page samples before the
file is published. Because Themida can destroy the original section table, the
result uses a flat PE layout with analysis sections derived from the measured
virtual-memory protection map (`PointerToRawData = RVA`),
preserves the header's original entry-point field, prints SHA-256 and timing
evidence, and stops the observed child. Dump mode applies no patch manifest,
does not signal GameGuard, and installs only the termination observer needed to
keep the process measurable.

Dump destinations inside the source repository and existing destination files
are rejected. Captures are local analysis artefacts and must remain outside
version control under the repository's clean-room rules.
