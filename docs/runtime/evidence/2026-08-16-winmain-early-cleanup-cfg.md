# WinMain early-cleanup CFG and launcher-tail evidence

- Analysis date: 2026-08-16 TRT
- Unpacked image: `%USERPROFILE%\Downloads\goley-unpacked-20260816-011037.dump`
  - Size: 22,642,688 bytes
  - SHA-256: `AA34F37364069652ED7CE6AAB105DD43BA7F61D7C09EC2A231B1B9E9D44D7BA1`
- Image base used below: `0x00400000`
- WinMain-style function: VA `0x00d3fab0`, RVA `0x0093fab0`
- Related exit-stack evidence:
  `docs/runtime/evidence/2026-08-16-exit-stack.md`
- Clean-run process input:
  `%USERPROFILE%\AppData\Local\Temp\goley-20260816-012720-clean75-pid.json`
  - SHA-256: `B97BEDE858E48FEEBE1B409CB32688AD0A840B6F35D2A687991B8F953D593AC4`

This was a read-only analysis. The unpacked image was decoded with Capstone
5.0.7 and iced-x86. No client byte, GameGuard file, patch manifest, or runtime
process was changed, and no target code was executed for this analysis.

## Result

There are two distinct ways for the early WinMain code to reach the zero-return
epilogue at `0x00d40eaa`:

1. The first initialization gate at `0x00d3fbbf` can fail. Its three
   locale-selected error blocks jump directly to `0x00d40eaa`.
2. The second initialization gate calls `0x00d35ab0`; its returned `AL` is
   tested at `0x00d3fc88`. A zero return takes the only conditional branch that
   directly targets `0x00d40eaa`.

There is also a larger common-cleanup head at `0x00d40e96`. Twelve direct jumps
enter that block from later initialization paths. It performs local cleanup and
falls through to `0x00d40eaa`. The current launcher command-line construction
selects one of those paths before the normal client initialization gate at
`0x00d3f420` can run: it quotes `TRAuth`, while this WinMain compares the raw
tail against an unquoted `TRAuth` prefix.

The dump snapshot contains `00 00 00 00` at RVA `0x00ebdc00`, corresponding to
the runtime mode global at VA `0x012bdc00`. Thus the captured run's mode is
measured as `0`, not the Turkish mode value `2`.

This identifies a launcher-tail serialization defect, not a client patch site.
The exact last runtime predecessor still needs a hardware-breakpoint capture;
static reachability alone is not treated as proof that a branch was taken.

## Direct inbound edges to the zero-return sink

The direct-control xref set inside WinMain is complete:

| Source VA | Source RVA | Bytes | Edge | Condition/source |
| ---: | ---: | --- | --- | --- |
| `0x00d3fbde` | `0x0093fbde` | `E9 C7 12 00 00` | `JMP 0x00d40eaa` | first gate failed; mode `0` error block |
| `0x00d3fbf9` | `0x0093fbf9` | `E9 AC 12 00 00` | `JMP 0x00d40eaa` | first gate failed; mode `2` error block |
| `0x00d3fc0f` | `0x0093fc0f` | `E9 96 12 00 00` | `JMP 0x00d40eaa` | first gate failed; remaining-mode error block |
| `0x00d3fc8a` | `0x0093fc8a` | `0F 84 1A 12 00 00` | `JE 0x00d40eaa` | `AL == 0` from `0x00d35ab0` |
| `0x00d40ea5` | `0x00940ea5` | `E8 66 57 9D FF` | fall-through | call `0x00716610`, then `0x00d40eaa` |

There is exactly one direct conditional xref to the sink: the `JE` at
`0x00d3fc8a`. The other direct branch xrefs are unconditional.

The first gate and its error fan-out are:

```asm
00D3FBB7  E8 B4 EF FF FF       call 00D3EB70
00D3FBBC  83 C4 04             add  esp,4
00D3FBBF  85 C0                test eax,eax
00D3FBC1  75 51                jne  00D3FC14

00D3FBC9  3B C3                cmp  eax,ebx       ; mode 0?
00D3FBCB  75 16                jne  00D3FBE3
...                            ; display mode-0 error
00D3FBDE  E9 C7 12 00 00       jmp  00D40EAA

00D3FBE3  83 F8 02             cmp  eax,2         ; mode 2?
00D3FBE6  75 16                jne  00D3FBFE
...                            ; display mode-2 error
00D3FBF9  E9 AC 12 00 00       jmp  00D40EAA

00D3FBFE  ...                  ; display remaining-mode error
00D3FC0F  E9 96 12 00 00       jmp  00D40EAA
```

The code at `0x00d3eb70` obtains the module path, attempts to open that path
with the literal `rb`, closes the resulting stream on success, and returns
`EAX=1`; otherwise it returns `EAX=0`. Function/API names are not present in the
binary, so the file-open description is based on call shape and literals rather
than symbols.

## The `0x00d3fc88` gate

The complete local call sequence leading into the gate is:

| Call VA | Call RVA | Callee VA | Callee RVA | Observed role |
| ---: | ---: | ---: | ---: | --- |
| `0x00d3fc14` | `0x0093fc14` | `0x00d30ba0` | `0x00930ba0` | unchecked initializer |
| `0x00d3fc1b` | `0x0093fc1b` | `0x008b7f91` | `0x004b7f91` | allocates `0x6c` bytes |
| `0x00d3fc4c` | `0x0093fc4c` | `0x00d36fa0` | `0x00936fa0` | unchecked initializer |
| `0x00d3fc53` | `0x0093fc53` | `0x00d2ecb0` | `0x0092ecb0` | called with argument `1` |
| `0x00d3fc5b` | `0x0093fc5b` | `0x00b6a3c0` | `0x0076a3c0` | unchecked initializer |
| `0x00d3fc65` | `0x0093fc65` | `0x750ed290` | outside main image | runtime-resolved target in this snapshot |
| `0x00d3fc75` | `0x0093fc75` | `0x00d29850` | `0x00929850` | unchecked initializer |
| `0x00d3fc83` | `0x0093fc83` | `0x00d35ab0` | `0x00935ab0` | boolean initialization gate |

Immediately before the last call, the caller loads the implicit object in
`ESI`:

```asm
00D3FC7A  8B 15 C4 C9 2B 01    mov  edx,[012BC9C4]
00D3FC80  8B 72 1C             mov  esi,[edx+1C]
00D3FC83  E8 28 5E FF FF       call 00D35AB0
00D3FC88  84 C0                test al,al
00D3FC8A  0F 84 1A 12 00 00    je   00D40EAA
```

`0x00d35ab0` has one explicit false return and one explicit true return:

```asm
00D35C9C  39 9E 04 01 00 00    cmp  dword ptr [esi+104h],ebx ; EBX=0
00D35CA2  75 23                jne  00D35CC7
...                            ; local destruction
00D35CC0  32 C0                xor  al,al
00D35CC2  E9 78 0A 00 00       jmp  00D3673F

00D3673D  B0 01                mov  al,1
00D3673F  ...                   ; common epilogue
00D36759  C3                   ret
```

The field at `[ESI+0x104]` is the length of a small-string object rooted at
`ESI+0xf4`. The function builds these three candidates:

| Index | Literal VA | Literal | From child CWD `C:\Joygame\Goley\BinaryTr` |
| ---: | ---: | --- | --- |
| 0 | `0x00f8a24c` | `Data/` | `C:\Joygame\Goley\BinaryTr\Data` — absent |
| 1 | `0x00f8a254` | `../Data/` | `C:\Joygame\Goley\Data` — present |
| 2 | `0x00f8a260` | `../../BinNew/Data/` | `C:\Joygame\BinNew\Data` — absent |

At `0x00d35bef`, callee `0x008b9a91` is invoked as `(candidate, 0)` and its
result is compared with `-1` at `0x00d35bf7`. The first candidate whose result
is not `-1` is copied into the `ESI+0xf4` string. After all three fail, its
length remains zero and the explicit `AL=0` path is taken. The `_access`-like
identity of `0x008b9a91` is an inference; the arguments, return comparison,
candidate literals, copy, and length test are measured instructions.

## Common-cleanup inbound edges

Later initialization paths converge at `0x00d40e96`, then call the local
destructor at `0x00716610` and fall through to the zero-return sink:

```asm
00D40E96  C7 84 24 08 01 00 00 FF FF FF FF
          mov dword ptr [esp+108h],-1
00D40EA1  8D 4C 24 12         lea  ecx,[esp+12h]
00D40EA5  E8 66 57 9D FF      call 00716610
00D40EAA  33 C0               xor  eax,eax
```

All twelve direct jumps to `0x00d40e96` are:

| Jump VA | Jump RVA | Immediate controlling observation |
| ---: | ---: | --- |
| `0x00d3fddd` | `0x0093fddd` | `0x00d3fdbe`: command line does not contain `NoPopup`; error is displayed |
| `0x00d3fe22` | `0x0093fe22` | mode-0 fallback object allocation succeeded |
| `0x00d3fe37` | `0x0093fe37` | mode-0 fallback object allocation returned null |
| `0x00d4005e` | `0x0094005e` | corresponding mode-2 `NoPopup` search returned null; error displayed |
| `0x00d400a7` | `0x009400a7` | mode-2 fallback object allocation succeeded |
| `0x00d402a6` | `0x009402a6` | corresponding mode-3 `NoPopup` search returned null; error displayed |
| `0x00d402ef` | `0x009402ef` | mode-3 fallback object path |
| `0x00d40467` | `0x00940467` | corresponding mode-4 `NoPopup` search returned null; error displayed |
| `0x00d40770` | `0x00940770` | corresponding mode-5 `NoPopup` search returned null; error displayed |
| `0x00d407ca` | `0x009407ca` | mode-5 fallback object path |
| `0x00d40ac7` | `0x00940ac7` | common initializer `0x00d3f420` returned `AL=0` |
| `0x00d40b4b` | `0x00940b4b` | callee `0x008e1ad0` returned `EAX=0` |

The instruction at `0x00d40e91` also falls through naturally into
`0x00d40e96` after normal teardown. Therefore a breakpoint only at
`0x00d40eaa` does not distinguish direct early return, common early cleanup,
or normal teardown.

## Launcher-tail mismatch

The clean-run process record contains this exact command line:

```text
"\\?\C:\Joygame\Goley\BinaryTr\BinaryTr.bin" "TRAuth" "NoPopup"
```

`client_command_line` in `crates/goley-boot/src/windows_process.rs` constructs
that value by calling `push_quoted_argument` for the image path, region, and
`NoPopup` independently.

CRT helper `0x008d5d80` scans the executable token, tracks quotes only while it
finds that token's end, skips following whitespace, and returns the next raw
character. It does not strip quotes around the next argument. For the measured
command line, WinMain therefore receives this raw tail:

```text
"TRAuth" "NoPopup"
^
EDI points to 0x22 here
```

WinMain's mode selection compares that raw pointer with unquoted two-byte
literals:

| Compare call | Literal | Equal-path effect |
| ---: | --- | --- |
| `0x00d3fb06` | `NM` | mode remains/sets `0` |
| `0x00d3fb20` | `KR` | mode remains/sets `0` |
| `0x00d3fb38` | `TR` | `0x00d3fb44` writes mode `2` |
| `0x00d3fb58` | `ID` | writes mode `3` |
| `0x00d3fb78` | `VN` | writes mode `4` |
| `0x00d3fb98` | `GL` | writes mode `5` |

Because `0x22` does not match `T` (`0x54`), the Turkish compare is nonzero and
the write at `0x00d3fb44` is skipped. The later six-byte `TRAuth` comparison at
`0x00d3fe49` has the same problem. The command line should be measured with the
launcher executable path quoted but these fixed, whitespace-free control
tokens unquoted:

```text
"C:\Joygame\Goley\BinaryTr\BinaryTr.bin" TRAuth NoPopup
```

This is a source-level launcher contract correction to validate, not a client
byte patch. No `patches.toml` entry follows from this finding.

## Next hardware-breakpoint sets

Use runtime addresses computed as `reported_image_base + RVA`; the listed VAs
assume the measured base `0x00400000`.

### Pass A: confirm raw command tail and first two gates

| Priority | VA | RVA | Capture before continuing |
| ---: | ---: | ---: | --- |
| 1 | `0x00d3faf3` | `0x0093faf3` | `EDI`, then at least 64 bytes at `EDI`; this is immediately after loading `lpCmdLine` |
| 2 | `0x00d3fb40` | `0x0093fb40` | `EAX`, `EDI`, and global dword `0x012bdc00`; this is the return from the `TR` prefix compare |
| 3 | `0x00d3fbbf` | `0x0093fbbf` | `EAX`; first initializer return before `TEST EAX,EAX` |
| 4 | `0x00d3fdbc` | `0x0093fdbc` | `EAX`; `NoPopup` substring-search result on the mode-0 path |

If `0x00d3fb40` observes `EAX != 0` with byte `0x22` at `EDI`, the launcher-tail
mismatch is dynamically confirmed. If `0x00d3fdbc` then observes nonzero, the
expected quiet fallback is `0x00d3fde2` followed by `0x00d3fe22` or
`0x00d3fe37`, then common cleanup.

### Pass B: distinguish the data-directory gate

| Priority | VA | RVA | Capture before continuing |
| ---: | ---: | ---: | --- |
| 1 | `0x00d35bf7` | `0x00935bf7` | `EAX`, `[EBP-0x3ac]` candidate index, and candidate string; may hit three times |
| 2 | `0x00d35c9c` | `0x00935c9c` | `ESI`, strings at `ESI+0xd8` and `ESI+0xf4`, lengths at `ESI+0xe8` and `ESI+0x104` |
| 3 | `0x00d3fc88` | `0x0093fc88` | `EAX/AL`, all GPRs, caller stack, and 64 bytes of precondition code |
| 4 | `0x00d40eaa` | `0x00940eaa` | stack/call trace and hit order; sink only, not a predicate |

### Pass C: after launcher-tail correction

The common initializer at `0x00d3f420` has four explicit false-return sources
and a single explicit true return:

| Post-call test VA | Callee VA | False block | Success continuation |
| ---: | ---: | ---: | ---: |
| `0x00d3f598` | `0x00d3b320` | `AL=0` at `0x00d3f5d6` | `0x00d3f5dd` |
| `0x00d3f5f5` | `0x00d89e20` | `AL=0` at `0x00d3f621` | `0x00d3f628` |
| `0x00d3f8e0` | `0x00c64ea0` | `AL=0` at `0x00d3f90d` | `0x00d3f914` |
| `0x00d3f9c5` | `0x00d374c0` | `AL=0` at `0x00d3f9e3` | `0x00d3f9ea` |

Success is `MOV AL,1` at `0x00d3fa86`. WinMain calls this initializer at
`0x00d40ab6`, tests the result at `0x00d40abe`, and takes `0x00d40ac7` into
common cleanup when it is false. The four post-call test addresses above fit
exactly in the four x86 hardware-breakpoint slots and identify the first failed
callee without modifying code bytes.

