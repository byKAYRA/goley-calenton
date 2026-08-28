# Launcher command-line A/B evidence

- Capture date: 2026-08-16 TRT
- Client: `C:\Joygame\Goley\BinaryTr\BinaryTr.bin`
- Client SHA-256 in both runs:
  `C136E751905FBE60CF27A71D7D05FBDE7C0428484D577F3A0A56766C839EFCFA`
- Runtime image base: `0x00400000`
- Measured comparison site: VA `0x00D3FB40`, RVA `0x0093FB40`
- Locale-mode global: VA `0x012BDC00`

## Live A/B result

| Run | Client command line | `TR` comparison | Resulting mode |
| --- | --- | ---: | ---: |
| `20260816-0157-phase1-gate3`, PID `29332` | `"\\?\C:\Joygame\Goley\BinaryTr\BinaryTr.bin" "TRAuth" "NoPopup"` | `EAX = 0xFFFFFFCE` (`-50`) | `0` |
| `20260816-0202-phase1-unquoted`, PID `22904` | `"\\?\C:\Joygame\Goley\BinaryTr\BinaryTr.bin" TRAuth NoPopup` | `EAX = 0x00000000` | `2` |

The quoted run exposed `"TRAuth" "NoPopup"` as the raw WinMain tail. Its
first byte was `0x22` (`"`), so the two-byte comparison with `TR` returned
`-50`; the conditional path skipped the write of mode `2`, and the later live
read found mode `0`.

The unquoted run exposed `TRAuth NoPopup` as the raw tail. The same comparison
returned zero, the fall-through at `0x00D3FB44` wrote `2` to
`0x012BDC00`, and the later live read found mode `2`.

## Build identity and decision

| Artifact | Quoted run SHA-256 | Unquoted run SHA-256 |
| --- | --- | --- |
| `goley-boot.exe` | `EBA3363F4BCC9E7A6B58EBD5C8FEC46E8AA80DF8B9C96BB6FA883B05560E191D` | `E8F7565839BB7D9272E185BC46F13E4F47C7D3A1AEFE3360FF05661026F7A121` |
| `goley_shim.dll` | `07FA308B65877AACAFF39A0A19502AB3F134B665F509B62340F6ED686DACC84A` | `07FA308B65877AACAFF39A0A19502AB3F134B665F509B62340F6ED686DACC84A` |
| `BinaryTr.bin` | `C136E751905FBE60CF27A71D7D05FBDE7C0428484D577F3A0A56766C839EFCFA` | `C136E751905FBE60CF27A71D7D05FBDE7C0428484D577F3A0A56766C839EFCFA` |

Both shim traces report `applied: 0` for the client patch manifest. The client
and shim hashes are identical across the A/B runs; only the launcher build
changed. Therefore this is a `goley-boot` argv-serialization correction:
retain quoting for the executable path, but emit the validated,
whitespace-free control tokens `TRAuth` and `NoPopup` without quotes. It is not
a client-byte patch, and it does not create a `patches.toml` entry.

## Run artifacts

- Quoted launch record:
  `%USERPROFILE%\AppData\Local\Temp\goley-20260816-0157-phase1-gate3-launch.json`
- Quoted gate metadata:
  `%USERPROFILE%\AppData\Local\Temp\goley-phase1-20260816-0157.release.metadata.json`
- Quoted shim trace:
  `%USERPROFILE%\AppData\Local\Temp\goley-20260816-0157-phase1-gate3-shim.jsonl`
- Unquoted launch record:
  `%USERPROFILE%\AppData\Local\Temp\goley-20260816-0202-phase1-unquoted-launch.json`
- Unquoted gate metadata:
  `%USERPROFILE%\AppData\Local\Temp\goley-phase1-20260816-0202.release.metadata.json`
- Unquoted shim trace:
  `%USERPROFILE%\AppData\Local\Temp\goley-20260816-0202-phase1-unquoted-shim.jsonl`

