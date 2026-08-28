# Login reached with measured GameGuard-380 compatibility gate

- Capture date: 2026-08-16 TRT
- Accepted run tag: `20260816-164000-gg380-compat-delayed`
- Client: `C:\Joygame\Goley\BinaryTr\BinaryTr.bin`
- Client SHA-256:
  `C136E751905FBE60CF27A71D7D05FBDE7C0428484D577F3A0A56766C839EFCFA`
- Verdict: **the Turkish username/password login screen was reached.**
- Next blocker: the login screen subsequently displayed
  `Gameguard Error99(0)`. Error 99(0) was recorded but not analyzed in this
  run; connecting or submitting login is a separate task.

No debugger was attached (`X32dbgClosed: true`). No identity was supplied,
no login request was submitted, and no auth/server reply was forged.

## Exact launcher environment and command

The official launcher-parameter envelope inherited by the elevated launcher
was:

```powershell
$env:NMRunEnv_VER = '0'
$env:NMRunEnv_ENUM = 'NMRunEnv_DATA_1'
$env:NMRunEnv_DATA_1 = 'fc2329727b4856f29186165dcc4dfe822fa1c2959e48f21aa738aedfb42c1c2ccaaafb29cb7efa641dbe726dd07a810241ac4b6b5edb2c305d473a0f5f8b6386206d0f4b985160b36d662872e5537ea86f685615565cf9bd3739018742b38d23'
```

It corresponds to the matching command-tail key
`NMRP20260816LOCALKEY0001`; the nonempty launcher parameters were
`SERVER_IP=213.74.179.12`, `SERVICE=Real`, and `OVERRIDE_LANGUAGE=TK`.
`USER_ID` and `USER_PW` were empty and therefore absent from the serialized
payload.

The exact command recorded in the status JSON was run elevated from
`<ProjectRoot>`:

```powershell
& '<ProjectRoot>\target\i686-pc-windows-msvc\release\goley-boot.exe' run `
  --client 'C:\Joygame\Goley\BinaryTr\BinaryTr.bin' `
  --region TRAuth `
  --runparam-key NMRP20260816LOCALKEY0001 `
  --oep-rva 0x009374DB `
  --late-inject-ms 3000 `
  --shim '<ProjectRoot>\target\i686-pc-windows-msvc\release\goley_shim.dll' `
  --patches '%USERPROFILE%\AppData\Local\Temp\goley-gg380-compat-patches.toml' `
  --timeout 150 -vv
```

Artifact identities recorded by that run:

| Artifact | SHA-256 |
| --- | --- |
| `goley-boot.exe` | `D9A3624AD94C5E04DCEF8CC93867AF3BDF761A56663B0C6E9B0100719333A98F` |
| `goley_shim.dll` | `0F35DAE70A0ECE8D4D2AB117A9C0D4827FD1354BB2BA10B955B51EE3FE65DD23` |
| temporary patch manifest | `E3FF1B2DD275BF0578E9F7D4D317A38EB7AA076E2E7855F763E6E422C1475D34` |

## Measured compatibility patch

The preceding launcher-valid control had reached GameGuard error 380. The
compatibility run used the exact GameGuard-init comparison already measured
for this client build:

| Field | Value |
| --- | --- |
| RVA / VA | `0x009374DB` / `0x00D374DB` |
| Original bytes | `81 FE 55 07 00 00` (`cmp esi, 0x755`) |
| Patched bytes | `81 FE 7C 01 00 00` (`cmp esi, 0x17C`, decimal 380) |
| Build guard | `C136E751905FBE60CF27A71D7D05FBDE7C0428484D577F3A0A56766C839EFCFA` |

The raw shim log records `static_patch` at decimal RVA `9663707`, followed by
`patch_summary.applied = 1`. Thus the six original bytes and build hash were
verified before the write. `--late-inject-ms 3000` was required so the
verified patch stage ran after the target code had materialized; the option
name `--oep-rva` does not establish that this comparison is the client's true
OEP.

## Window and visual evidence

The PID-scoped telemetry contains 371 samples for client PID `16320`:

1. At `25,951 ms`, the first visible target window appeared as
   `ChaguClass`, title `Goley V32599`, at `674x336`.
2. At `40,633 ms`, that window first occupied `1920x1080` fullscreen.
3. At `55,814 ms`, a visible `#32770` dialog titled `GameGuard Error`
   appeared while the fullscreen client remained present.
4. The 75-second screenshot visibly contains the Turkish `Giriş` panel with
   `Joygame Kullanıcı Adı` and `Şifre` fields, together with the dialog text
   `Gameguard Error99(0)`. This is the positive login milestone and the exact
   next blocker.

Relevant screenshots remain outside the repository:

| Stage | Absolute path | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| launcher splash | `%USERPROFILE%\Downloads\goley-20260816-164000-gg380-compat-delayed-30s.png` | 867,827 | `DE9992F69AAA4BBECA898B4C9F833E0651C02D783EBE8BEDF3ACC37400F368E5` |
| AniPark intro | `%USERPROFILE%\Downloads\goley-20260816-164000-gg380-compat-delayed-45s.png` | 168,622 | `E9C3561AB7CE34A78A8CC2FEB78D7D00DB8A3AEED9B3DAA30D858B8CC8C90139` |
| login plus Error 99(0) | `%USERPROFILE%\Downloads\goley-20260816-164000-gg380-compat-delayed-75s.png` | 2,380,868 | `C30C35240714378569E45E71B16578B12B640BA5FDB30140E8E1FADC5A88DB9F` |

## Preservation and primary artifacts

After the bounded 150-second observation, the runtime GameGuard directory
was backed up and the canonical directory was restored from the pristine
source. The final comparison records 32 pristine files, 32 canonical files,
and zero content differences.

All runtime artifacts remain outside the repository:

| Artifact | Absolute path | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| Status and exact command | `%USERPROFILE%\AppData\Local\Temp\goley-20260816-164000-gg380-compat-delayed-status.json` | 8,847 | `EB57999B3EBBFA34FB472F11305075A2318DBF2B36A50A6A5B5A3CD894D429AC` |
| Raw shim JSONL | `%USERPROFILE%\AppData\Local\Temp\GoleyBoot-14632-18cc4cadd1f3a6f0-1-ready.jsonl` | 11,793 | `227679D3EBBF31FCE0CC4A77AD428E0FB9B1F9651A1031BAAE216D9E8A46DD28` |
| PID-scoped window telemetry | `%USERPROFILE%\AppData\Local\Temp\goley-20260816-164000-gg380-compat-delayed-telemetry.jsonl` | 210,893 | `CB1FF4DD44BF6A9A9EF669CB71EE86F9DA8D68C857030BE272AD382DF07AF1F2` |
| GameGuard restore record | `%USERPROFILE%\AppData\Local\Temp\goley-20260816-164000-gg380-compat-delayed-gameguard.json` | 618 | `A4428B6C410B975964A397280D6FDC6E527D3FAB6AB0032726ECF8254FFBF49E` |
| Launcher stdout | `%USERPROFILE%\AppData\Local\Temp\goley-20260816-164000-gg380-compat-delayed-boot-stdout.txt` | 392 | `D4AAB59B64FA21478AC2E4715D1C475819D63F2A951A6CAEC075C3089CA4307B` |

The accepted run's ten primary artifacts were also copied byte-for-byte from
their temporary/download locations to the stable, non-repository archive
`%USERPROFILE%\Games\Goley-Evidence\2026-08-16-login-reached`. The archive
index is `SHA256.json` (SHA-256
`B5C40A670E52467486C584A73BEC6537409EA8412222FA06DC077DF12D413429`).
Each copied file was hash-verified against its source before the index was
written.

## Persisted reproduction state

After the accepted run, the same build-keyed patch was preserved at
`<ProjectRoot>\crates\goley-shim\patches\patches.toml`
(SHA-256 `1C89560A0B824FC3C82BCECAE5A8B4D1C5341419DFB0312A8696426D37E03A5D`).
For subsequent reproduction, replace the temporary `--patches` argument in
the recorded command with this repository path. The i686 `goley-shim` test
suite (12 total unit/integration tests), `cargo fmt --check`, and clippy with
warnings denied all passed after persistence.

## Decision

The former "login not reached" result for `TRAuth NoPopup` is superseded for
the valid launcher-envelope path plus the measured error-380 compatibility
gate. The client now reaches and draws its own login UI. Work stops at this
milestone; `Gameguard Error99(0)` is the first unresolved post-login blocker
for the next run.
