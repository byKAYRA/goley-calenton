# BinaryTr exit-stack postmortem

- Local analysis date: 2026-08-16 TRT
- WER minidump: `%USERPROFILE%\AppData\Local\CrashDumps\BinaryTr.bin.29612.dmp`
  - Size: 24,109,649 bytes
  - SHA-256: `8338027D08D6F238809C454C6A8FADE866FE1B932618B200858A0FE24A8CAF8B`
- Independently captured unpacked image: `%USERPROFILE%\Downloads\goley-unpacked-20260816-011037.dump`
  - Size: 22,642,688 bytes (`SizeOfImage = 0x1598000`)
  - SHA-256: `AA34F37364069652ED7CE6AAB105DD43BA7F61D7C09EC2A231B1B9E9D44D7BA1`
- Loaded client image in the minidump: base `0x00400000`, size `0x1598000`

This is a read-only postmortem. The
[MINIDUMP_EXCEPTION_STREAM](https://learn.microsoft.com/en-us/windows/win32/api/minidumpapiset/ns-minidumpapiset-minidump_exception_stream)
and related streams were parsed directly, and the recovered x86 bytes were
decoded with Capstone 5.0.7. No client, GameGuard, patch manifest, or repository
source was changed. The unpacked image came from a separate no-debugger run and
is used only to cross-check the WER memory.

## Finding

The primary event is a normal CRT shutdown request, `ExitProcess(0)`. Microsoft's
[`ExitProcess`](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-exitprocess)
contract has no return value and terminates the calling process. The WER
`0x80000003` exception at `BinaryTr.bin+0x4ba17d` is a secondary diagnostic
effect caused by the observation shim returning from an API that is normally
non-returning:

1. the client WinMain-style function returns `0`;
2. the CRT startup passes that value through its `exit`/`doexit` path;
3. the client calls the x86 `kernel32!ExitProcess` export with status `0`;
4. exitguard suppresses that request and returns for observation;
5. execution reaches the explicit `int3` immediately after the non-returning
   call, producing the WER breakpoint exception.

Consequently, `+0x4ba17d` is not the predicate that made startup give up and it
is not a justified patch site. The earlier predicate inside WinMain remains
**unknown**.

## Exception context

The minidump exception stream selects thread `20648` (`0x50a8`) and contains a
full x86 context (`ContextFlags = 0x0001007f`):

| Field | Value |
|---|---:|
| Exception code | `0x80000003` (`STATUS_BREAKPOINT`) |
| Exception flags | `0x00000000` |
| Exception address | `0x008ba17d` = `BinaryTr.bin+0x4ba17d` |
| Exception parameters | 1; value `0x00000000` |
| EIP | `0x008ba17d` |
| ESP | `0x001afd38` |
| EBP | `0x001afd38` |
| EAX | `0x001afd74` |
| EBX | `0x06a98780` |
| ECX | `0x3e959288` |
| EDX | `0x00000000` |
| ESI | `0x06c60000` |
| EDI | `0x06a9877c` |
| EFlags | `0x00200212` |
| CS / SS | `0x0023` / `0x002b` |
| DR0 / DR1 / DR2 / DR3 | all `0` |
| DR6 / DR7 | both `0` |

The trap flag is clear and the hardware-debug registers are zero. More
importantly, the `CC` at this RVA also exists in the independently captured
unpacked image. It is client code, not a debugger-inserted software breakpoint.

## ExitProcess call and deliberate post-call trap

The WER memory and the independent unpacked image have identical bytes in the
64-byte window `BinaryTr.bin+0x4ba15d..+0x4ba19c`. The relevant function is:

```asm
BinaryTr.bin+0x4ba168  55                 push ebp
BinaryTr.bin+0x4ba169  8B EC              mov  ebp, esp
BinaryTr.bin+0x4ba16b  FF 75 08           push dword ptr [ebp+8]
BinaryTr.bin+0x4ba16e  E8 C8 FF FF FF     call BinaryTr.bin+0x4ba13b
BinaryTr.bin+0x4ba173  59                 pop  ecx
BinaryTr.bin+0x4ba174  FF 75 08           push dword ptr [ebp+8]
BinaryTr.bin+0x4ba177  90                 nop
BinaryTr.bin+0x4ba178  E8 33 0F 84 74     call 0x750fb0b0
BinaryTr.bin+0x4ba17d  CC                 int3
```

In this minidump, x86 `kernel32.dll` is based at `0x750d0000`. The matching
local `C:\Windows\SysWOW64\kernel32.dll` has the same PE timestamp
(`0x44ad50f4`) and `SizeOfImage` (`0xf0000`) as the minidump module. Its export
table identifies RVA `0x2b0b0`, hence absolute `0x750fb0b0`, as `ExitProcess`.
The stack value `[EBP+8]` at the call is `0`.

The `int3` is therefore the compiler/runtime guard after a call expected never
to return. It became reachable only because exitguard suppressed
`ExitProcess(0)` for diagnostics.

## Exact EBP frame chain

The first three saved frame pointers form an unambiguous client return chain.
The outer startup function uses a compiler SEH prologue, so unwinding stops
there rather than treating its `[EBP+4]` value as another code return:

| Frame | EBP | Saved EBP | Return address / state | Meaning |
|---:|---:|---:|---:|---|
| 0 | `0x001afd38` | `0x001afd84` | `BinaryTr.bin+0x4ba3a9` | Return from the `ExitProcess` wrapper into the CRT `doexit`-style routine |
| 1 | `0x001afd84` | `0x001afd98` | `BinaryTr.bin+0x4ba3cf` | Return from `doexit` into the `exit` wrapper |
| 2 | `0x001afd98` | `0x001afe1c` | `BinaryTr.bin+0x4bd6fb` | Return from `exit(WinMain_result)` into CRT startup |
| 3 | `0x001afe1c` | `0x00000000` | `[EBP+4]` is not a module address | CRT startup's compiler SEH frame; reliable EBP unwind terminates |

The status is preserved as zero through every layer:

| Location | Observed value |
|---|---:|
| CRT startup `[0x001afe1c-0x20]` (saved WinMain result) | `0` |
| `exit` wrapper `[0x001afd98+8]` | `0` |
| `doexit` frame `[0x001afd84+8]` | `0` |
| `doexit` flags `[EBP+0x0c]`, `[EBP+0x10]` | `0`, `0` |
| `ExitProcess` wrapper `[0x001afd38+8]` | `0` |

The matching call sites explain all three client return addresses:

```asm
; doexit-style tail
BinaryTr.bin+0x4ba3a1  FF 75 08           push dword ptr [ebp+8] ; status
BinaryTr.bin+0x4ba3a4  E8 BD FD FF FF     call BinaryTr.bin+0x4ba166
BinaryTr.bin+0x4ba3a9  83 7D 10 00        cmp  dword ptr [ebp+0x10], 0

; exit wrapper
BinaryTr.bin+0x4ba3c0  55                 push ebp
BinaryTr.bin+0x4ba3c1  8B EC              mov  ebp, esp
BinaryTr.bin+0x4ba3c3  6A 00              push 0
BinaryTr.bin+0x4ba3c5  6A 00              push 0
BinaryTr.bin+0x4ba3c7  FF 75 08           push dword ptr [ebp+8] ; WinMain result
BinaryTr.bin+0x4ba3ca  E8 AF FE FF FF     call BinaryTr.bin+0x4ba27e
BinaryTr.bin+0x4ba3cf  83 C4 0C           add  esp, 0x0c
```

The routine at `+0x4ba27e` walks CRT termination callback tables before the
shown tail. The semantic labels above are inferred from that implementation
and its callers; the binary has no surviving symbols.

## CRT startup proves this is the WinMain return path

The function at `BinaryTr.bin+0x4bd5d3` is an MSVC-style GUI CRT startup, not a
GameGuard decision function. The local x86 `kernel32.dll` export map confirms
its direct calls to `GetStartupInfoW` and `GetCommandLineA`. It also validates
the DOS/PE32 headers and checks PE data-directory slot 14 before selecting the
native versus managed shutdown path. The four pushed values match the official
[`WinMain`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-winmain)
argument order and types.

Its decisive sequence is:

```asm
BinaryTr.bin+0x4bd5e9  33 F6              xor  esi, esi        ; zero
...
BinaryTr.bin+0x4bd682  E8 89 FE 82 74     call kernel32!GetCommandLineA
...
BinaryTr.bin+0x4bd6cc  E8 AF 86 01 00     call BinaryTr.bin+0x4d5d80
                                         ; skip executable name, return lpCmdLine
BinaryTr.bin+0x4bd6e0  51                 push ecx             ; nCmdShow
BinaryTr.bin+0x4bd6e1  50                 push eax             ; lpCmdLine
BinaryTr.bin+0x4bd6e2  56                 push esi             ; hPrevInstance = 0
BinaryTr.bin+0x4bd6e3  68 00 00 40 00     push 0x00400000      ; hInstance
BinaryTr.bin+0x4bd6e8  E8 C3 23 48 00     call BinaryTr.bin+0x93fab0
BinaryTr.bin+0x4bd6ed  89 45 E0           mov  [ebp-0x20], eax ; save WinMain result
BinaryTr.bin+0x4bd6f0  39 75 E4           cmp  [ebp-0x1c], esi ; managed-image flag
BinaryTr.bin+0x4bd6f3  75 06              jne  BinaryTr.bin+0x4bd6fb
BinaryTr.bin+0x4bd6f5  50                 push eax             ; exit code
BinaryTr.bin+0x4bd6f6  E8 C3 CC FF FF     call BinaryTr.bin+0x4ba3be ; exit
BinaryTr.bin+0x4bd6fb  E8 EA CC FF FF     call BinaryTr.bin+0x4ba3ea ; cexit-style path
```

The called client function is at absolute `0x00d3fab0` (RVA `0x93fab0`). Its
first instructions access `[EBP+8]` and `[EBP+0x10]`, consistent with the four
arguments above. A recursive decode of the WER memory's direct-control CFG
through the next function boundary (`0x00d40ee0`) found 1,356 reachable
instructions, no indirect jumps, and exactly one reachable return:

```asm
BinaryTr.bin+0x940eaa  33 C0              xor  eax, eax
...
BinaryTr.bin+0x940ecc  8B E5              mov  esp, ebp
BinaryTr.bin+0x940ece  5D                 pop  ebp
BinaryTr.bin+0x940ecf  C2 10 00           ret  0x10
```

`ret 0x10` matches four 32-bit WinMain arguments, and `xor eax,eax` fixes its
return value to zero. The saved `[startup_EBP-0x20] = 0` in the WER stack is the
dynamic confirmation. Thus the measured path is:

```text
WinMain-style client function returns 0
  -> CRT startup calls exit(0)
  -> doexit invokes ExitProcess(0)
  -> exitguard returns
  -> explicit int3 at BinaryTr.bin+0x4ba17d
```

## Predicate status and patch decision

**Established:** the client completed a WinMain return path and deliberately
requested graceful process exit with status zero. The breakpoint exception is
post-suppression fallout.

**Unknown:** which earlier conditional inside
`BinaryTr.bin+0x93fab0..+0x940ecf` selected the startup-cleanup path. The dead
launcher service, GameGuard status, integrity logic, and other prerequisites
remain candidates only; this stack cannot distinguish them.

No `patches.toml` entry is warranted. The next discriminating measurement is a
clean-run branch trace scoped to the WinMain range, ending at
`BinaryTr.bin+0x4bd6ed` (the instruction immediately after WinMain returns).
The last taken conditional that enters the common cleanup/epilogue, together
with the value source it tests, will identify the real predicate. Patching
`+0x4ba17d`, the CRT `exit` wrapper, or the CRT startup would only hide the
symptom and discard the predicate evidence.
