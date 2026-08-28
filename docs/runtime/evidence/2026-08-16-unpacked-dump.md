# Unpacked client dump evidence

- Capture date: 2026-08-16 TRT
- Command: `goley-boot dump-unpacked`
- Input client: `C:\Joygame\Goley\BinaryTr\BinaryTr.bin`
- Input client SHA-256:
  `C136E751905FBE60CF27A71D7D05FBDE7C0428484D577F3A0A56766C839EFCFA`
- Output:
  `%USERPROFILE%\Downloads\goley-unpacked-20260816-011037.dump`
- Output size: `22,642,688` bytes (`0x01598000`)
- Output SHA-256:
  `AA34F37364069652ED7CE6AAB105DD43BA7F61D7C09EC2A231B1B9E9D44D7BA1`
- Image base: `0x00400000`
- `SizeOfImage`: `0x01598000`

## Capture measurements

| Measurement | Value |
| --- | ---: |
| First executable-image transition after primary resume | `321 ms` |
| Required quiescence interval | `100 ms` |
| Final capture elapsed after primary resume | `1,535 ms` |
| Change samples | `5` |
| Maximum changed ranges in one sample | `7` |
| Zero-filled unreadable bytes | `0` |

## Rebuilt synthetic sections

These sections describe coalesced runtime memory ranges; they do not claim to
recover the packer's original section table.

| Section | RVA | Virtual size | Raw size | Access | Characteristics |
| --- | ---: | ---: | ---: | --- | ---: |
| `.m0000` | `0x00001000` | `0x01069000` | `0x01069000` | RWX | `0xE0000020` |
| `.m0001` | `0x0106A000` | `0x00348000` | `0x00348000` | RW | `0xC0000040` |
| `.m0002` | `0x013B2000` | `0x001E6000` | `0x001E6000` | RWX | `0xE0000020` |

`AddressOfEntryPoint` is intentionally `0x00000000`: no original entry point
was measured during this capture, so the writer recorded OEP as unknown rather
than guessing.

The dump remains outside the repository under `%USERPROFILE%\Downloads`.
No client file, mapped image, extracted asset, packed binary, or memory dump was
added to the workspace; this document records metadata only.

The associated raw observer trace is
`%USERPROFILE%\AppData\Local\Temp\GoleyBoot-28684-18cc1a1773f363c4-1-ready.jsonl`
(SHA-256
`C685DD63C389DB1E61BAB1FBD9F7EAA17A0CBFF9B5C037DD6983CF00CE5531E0`).

