# Goley normal-run blocker report

- Local time: 2026-08-16 00:13 TRT
- Client: `C:\Joygame\Goley\BinaryTr\BinaryTr.bin`
- SHA-256: `C136E751905FBE60CF27A71D7D05FBDE7C0428484D577F3A0A56766C839EFCFA`
- Mode/region: `run` / `TRAuth`
- Static patches applied: 0
- GameGuard ready-event selected: none
- Network redirect: disabled
- Login/auth forge: none

## Last observed sequence

| Order | Observation | Caller/result |
| ---: | --- | --- |
| 1 | `Global\MtxNPGL` wait | `BinaryTr.bin+0x4e7827`; 9500 ms requested; returned `WAIT_OBJECT_0` (`0`) |
| 2 | `Global\MtxNPGM` created | `BinaryTr.bin+0x4e78c4`; no named Event followed |
| 3 | `ExitProcess(0)` suppressed | return site `BinaryTr.bin+0x4ba17d` |
| 4 | Current-process `TerminateProcess(..., 0xffffffff)` suppressed | `BugTrap.dll+0x10dc0` |
| 5 | Current-process `NtTerminateProcess(..., 0x80000003)` suppressed | `ntdll.dll+0x6e1e0` |
| 6 | Windows Application Error 1000 | exception `0x80000003`; faulting module offset `0x004ba17d` |

The mutex wait returned successfully and is not the startup blocker. No named
GameGuard ready Event was observed, so an event name was not guessed or
hard-coded.

## Result

No login window was observed. The exact last client-owned control site is
`BinaryTr.bin+0x4ba17d`: the client requested `ExitProcess(0)` there and, after
that request was suppressed for diagnostics, faulted at the same offset with a
breakpoint exception. The predicate that led to this termination path remains
unknown; no byte patch was added without measured branch bytes.
