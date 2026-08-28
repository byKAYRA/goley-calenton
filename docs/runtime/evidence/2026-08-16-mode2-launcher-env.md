# Mode-2 launcher environment blocker evidence

- Analysis date: 2026-08-16 TRT
- Runtime probe tag: `20260816-152647-mode2-path-probes`
- Current decision: the corrected unquoted `TRAuth` prefix selects the intended
  mode-2 code, but `TRAuth NoPopup` does not satisfy the launcher's
  `NMRunParamDLL` payload contract. The client treats `NoPopup` as the payload
  key, the payload load returns nonzero, and the mode-2 quiet-fallback path
  reaches its common-cleanup edge.
- Login status update: the later accepted run
  `20260816-164000-gg380-compat-delayed` supplied `TRAuth <KEY24>` with the
  matching inherited `NMRunEnv_*` envelope and reached the Turkish login UI.
  See `docs/runtime/evidence/2026-08-16-login-reached.md`; the next observed
  blocker is `Gameguard Error99(0)`.

The earlier debugger-free result in
`docs/runtime/evidence/2026-08-16-login-clean-run.md` remains valid for the
input it measured: `TRAuth NoPopup` produced no login window. The evidence in
this file narrows that result to an incomplete launcher contract; it is not a
clean result for the environment-backed official contract.

## Scope and controls

This documentation pass was read-only. It inspected only the already-produced
JSON, text, and Markdown evidence under `%LOCALAPPDATA%\Temp`; it did not start
the client, load a runtime DLL, or read or modify a client, dump, or GameGuard
file.

The six earlier runtime probes were six fresh, independently launched runs,
one exact checkpoint per run. Each used the shim's register-preserving x86
DR0 execute gate. The metadata records the expected `0x80000004` exception,
the requested `EIP`, and `DR6.B0`; it also records that exception dispatch
completed before the snapshot wait. These are therefore exact primary-thread
contexts, not polling samples. They are combined below with the already
documented static CFG; they are not represented as a single-process trace.

| Input | SHA-256 |
| --- | --- |
| probe `goley-boot.exe` | `7E733D137C772B6CC734783351124D3BD71F23AE6CB68342420D3F0330D34273` |
| probe `goley_shim.dll` | `0F35DAE70A0ECE8D4D2AB117A9C0D4827FD1354BB2BA10B955B51EE3FE65DD23` |
| `BinaryTr.bin` | `C136E751905FBE60CF27A71D7D05FBDE7C0428484D577F3A0A56766C839EFCFA` |
| `patches.toml` | `FE41E5C33FD1324DF9AF2A386622321FCBB8A195FA2775FD26FE7FCA195A884A` |

Every raw shim trace reports `patch_summary.applied = 0`. The wrapper required
the relevant target/debugger processes to be absent, stopped Windhawk for the
probe set, restored its prior running state, and restored the canonical
GameGuard directory after every checkpoint. `final.json` records a final
32-versus-32 inventory with zero content differences and Windhawk `Running`.
Each process was deliberately stopped only after its requested snapshot, so
the probe run durations are not application-lifetime measurements.

## Client-side launcher contract

The read-only disassembly in the NMRunParam evidence establishes this flow:

```asm
00D3FE49  push 6
00D3FE4B  push "TRAuth"
00D3FE50  push edi
00D3FE51  call compare
00D3FE59  test eax,eax
...
00D3FE77  add  edi,7              ; skip "TRAuth "
...
00D3FEB7  push 1                  ; consume environment on success
00D3FEB9  push <entire tail after "TRAuth ">
00D3FEBE  call 008E2C20
00D3FEC3  cmp  eax,ebx            ; EBX is zero here
00D3FEC5  jne  00D3FFED
...
00D3FFED  xor  bl,bl              ; payload-load failure flag
```

`0x008E2C20` resolves `NMRunParamDLL_Load` from
`.\NMRunParamDLL.dll` and calls it as `Load(data_id, key, consume)`. Therefore
the text after `TRAuth ` is not a generic second launcher flag or ciphertext;
it is the key used to open a ciphertext envelope inherited through the
process environment. The measured official form is:

```text
TRAuth <24-byte-key>
```

The probe launcher supplied `TRAuth NoPopup` and did not construct a matching
`NMRunEnv_*` envelope. Consequently `NoPopup` became the candidate payload key.
The same text remains visible to the later substring search, which explains
why the client can both fail the payload parser and still select its quiet
fallback.

## Exact mode-2 path measurements

The image base was `0x00400000` in every run. The decisive register values are
from each checkpoint's `snapshot.json`:

| Order | VA / RVA | Exact observation | Control-flow meaning |
| ---: | --- | --- | --- |
| 1 | `0x00D3FE59` / `0x0093FE59` | `EAX = 0x00000000` | The six-byte `TRAuth` comparison succeeded. The existing unquoted-prefix correction is in effect. |
| 2 | `0x00D3FEC3` / `0x0093FEC3` | `EAX = 0x00000C1D` (`3101`) | `NMRunParamDLL_Load` returned nonzero, so `JNE 0x00D3FFED` takes the payload-failure path. No symbolic name is assigned to code `3101`; the measured branch only requires it to be nonzero. |
| 3 | `0x00D40014` / `0x00940014` | `BL = 0` (`EBX = 0`) | The payload-parser success flag remains false on entry to the mode-selected continuation. |
| 4 | `0x00D4003D` / `0x0094003D` | `EAX = 0x01CCA5C4`, nonzero | The mode-2 `NoPopup` substring search succeeded, choosing the quiet rather than message-display fallback. |
| 5 | `0x00D40081` / `0x00940081` | `ESI = 0x0740D848`, nonzero | The mode-2 fallback allocation succeeded. |
| 6 | `0x00D400A7` / `0x009400A7` | exact `EIP` hit | Control reached the mode-2 fallback's direct common-cleanup edge documented in `2026-08-16-winmain-early-cleanup-cfg.md`. |

The final checkpoint took about 25 seconds to reach; the other five took
about 1.3--1.5 seconds. That timing difference does not weaken the exact hit:
the post-unpack gate waited for the requested instruction and recorded the
original primary-thread context there. Since the wrapper stopped each run
after its snapshot, checkpoint 6 proves arrival at the cleanup jump, not the
execution of every downstream epilogue instruction in that same run.

The combined measured route is:

```text
TRAuth prefix accepted
  -> tail "NoPopup" passed as NMRunParam key
  -> Load returns 0xC1D
  -> parser flag BL=0
  -> mode-2 NoPopup search succeeds
  -> fallback object allocation succeeds
  -> 0x00D400A7 common-cleanup edge
```

This moves the immediate blocker upstream of the later data-directory and
network initialization candidates: the current launch never obtains a valid
launcher payload in the first place.

## Official `NMRunParamDLL` round trips

The independent evidence at
`%LOCALAPPDATA%\Temp\nmrunparam-readonly-analysis\evidence.md` directly called
the official 32-bit DLL; it did not substitute the MIT codec for the final
blob and did not invoke `RunProgram` or start the game. DLL identity:

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `C:\Joygame\Goley\BinaryTr\NMRunParamDLL.dll` | 263,712 | `575FDAC6B095FC1D7148672F6D1A9FBE879AB546B1FDF1DBEC75E6E549D568C8` |

For both the recovered official-address parameters and the local-emulator
variant, the measured official API sequence was:

1. `CreateData(source_id) -> 0`.
2. `SetParam` for `USER_ID`, `USER_PW`, `SERVER_IP`, `SERVICE`, and
   `OVERRIDE_LANGUAGE` -> `0` for every call.
3. `Save(source_id, key24) -> 0`, producing `NMRunEnv_VER`,
   `NMRunEnv_ENUM`, and `NMRunEnv_DATA_1`.
4. `CreateData(target_id) -> 0` and
   `Load(target_id, key24, 1) -> 0`.
5. `GetParamCount -> 3`; the names were exactly `OVERRIDE_LANGUAGE`,
   `SERVER_IP`, and `SERVICE`.
6. All three nonempty values matched exactly, the environment was consumed,
   and the harness reported `ROUNDTRIP=OK`.

Empty `USER_ID` and `USER_PW` calls were accepted by `SetParam`, but `Save`
did not serialize empty values. Their post-load `GetParam` result was `5105`
with a null output pointer. This is measured behavior and is compatible with
testing whether the client presents its own login UI; no identity or session
value was forged.

The local-emulator variant used `SERVER_IP=127.0.0.1`, `SERVICE=Real`, and
`OVERRIDE_LANGUAGE=TK`. Its official-DLL output was:

```text
COMMAND_TAIL=TRAuth NMRP20260816LOCALKEY0001
NMRunEnv_VER=0
NMRunEnv_ENUM=NMRunEnv_DATA_1
NMRunEnv_DATA_1=fc2329727b4856f29186165dcc4dfe822fa1c2959e48f21aa738aedfb42c1c2ccaaafb29cb7efa641dbe726dd07a810241ac4b6b5edb2c305d473a0f5f8b63866fb1d73b90a6634f454fb8d9543c1d9f184e3e6fda1a9b0d9ff7ef1e3f011518
```

Calling `Load(..., 1)` against those exact inherited values returned `0`,
recovered all three nonempty parameters byte-for-byte, and deleted the
envelope variables. This proves that a valid blob can be produced for the
next local clean run; it does not prove what the game does after loading it.

If elevation is involved, the three variables must be in the environment of
the actual elevated process that creates `BinaryTr.bin`. Preparing them only
in an unrelated unelevated orchestration process, or after child creation,
does not satisfy the measured contract.

## Decision and next acceptance test

No client-byte patch is indicated. The next test belongs in launcher wiring:

1. create the local payload with the official API or a byte-exact compatible
   implementation;
2. place its `NMRunEnv_*` values in the real child-creating process;
3. launch with the matching `TRAuth <KEY24>` tail, not `TRAuth NoPopup`;
4. repeat the pristine-GameGuard, no-external-debugger clean-run controls;
5. collect PID-scoped top-level-window telemetry and the existing termination
   telemetry for the complete observation period.

The first two discriminating checkpoints in that rerun are
`0x00D3FE59` (`EAX=0` expected for the prefix) and `0x00D3FEC3`
(`EAX=0` required for a successful payload load). A login-window verdict is
recorded only after that contract is observed live.

**Status: superseded by the accepted login-reaching run documented in
`docs/runtime/evidence/2026-08-16-login-reached.md`.**

## Evidence artifacts

All runtime and DLL-test artifacts remain outside the repository.

| Artifact | Absolute path | SHA-256 |
| --- | --- | --- |
| consolidated runtime result | `%USERPROFILE%\AppData\Local\Temp\20260816-152647-mode2-path-probes\final.json` | `18C7B67BA44152CE2443090A783A9BB79095F022A1E8434269CCC788C2D76103` |
| official-DLL analysis | `%USERPROFILE%\AppData\Local\Temp\nmrunparam-readonly-analysis\evidence.md` | `14EA4281A9148B4A4070DDD146A41E80D1B77485867CB2BDEEF649958FCC00AD` |
| official-address API round trip | `%USERPROFILE%\AppData\Local\Temp\nmrunparam-readonly-analysis\nmrunparam_payload.output.txt` | `02CBBCB1462505028E973CC4510ABC84EA2168875F8AB9C17D35DA7B90FFFD8C` |
| local-emulator API round trip | `%USERPROFILE%\AppData\Local\Temp\nmrunparam-readonly-analysis\nmrunparam_payload_local.output.txt` | `593A4EA2D712248B00D16EA1B2E01A0263BED6E4AA4B138277A9CD89E990F273` |
| client launcher-path disassembly | `%USERPROFILE%\AppData\Local\Temp\nmrunparam-readonly-analysis\client_trauth_disasm.output.txt` | `C6BB0C48DE76136D18EFC8F758DF0BAEC6DA187883CB60555B0CB4A4B1049C1C` |

The six per-checkpoint `snapshot.json` files are direct children of the
numbered directories under the consolidated runtime-result directory; their
paths and raw-log SHA-256 values are also retained in `final.json`.
