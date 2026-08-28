# Goley startup wait-object report

This report contains observations only; it does not select or hard-code a GameGuard object.

- Parsed records: 14
- Ignored lines: 7
- Unique object/wait call sites: 9
- Termination call sites: 3

| Blocking candidate | Operation | Object | Caller | Count | Last outcome |
|---|---|---|---|---:|---|
| no | WaitForSingleObject | Global\MtxNPGL | BinaryTr.bin+0x4e7826 | 2 | 0 |
| no | WaitForSingleObject | \Sessions\1\BaseNamedObjects\SM0:29612:168:WilStaging_02_p0 | d3d9.dll+0x726cc | 2 | 0 |
| no | CreateMutexA | Global\MtxNPGL | BinaryTr.bin+0x4e77a4 | 1 | <unknown> |
| no | CreateMutexA | Global\MtxNPGM | BinaryTr.bin+0x4e78c4 | 1 | <unknown> |
| no | CreateMutexA | NL59NPGL | BinaryTr.bin+0x4e433d | 1 | <unknown> |
| no | CreateMutexA | NL59NPGL | BinaryTr.bin+0x4e4b07 | 1 | <unknown> |
| no | CreateMutexW | WindhawkSession4220\ProcessInitAPCMutex-pid=2668 | windhawk.dll+0x1f41a | 1 | <unknown> |
| no | CreateMutexW | WindhawkSession4220\ProcessInitAPCMutex-pid=29048 | windhawk.dll+0x1f41a | 1 | <unknown> |
| no | CreateMutexW | WindhawkSession4220\ProcessInitAPCMutex-pid=31308 | windhawk.dll+0x1f41a | 1 | <unknown> |

## Termination observations

| API | Status | Caller | Count |
|---|---|---|---:|
| ExitProcess | 0 | BinaryTr.bin+0x4ba17d | 1 |
| NtTerminateProcess | -2147483645 | ntdll.dll+0x6e1e0 | 1 |
| TerminateProcess | 4294967295 | BugTrap.dll+0x10dc0 | 1 |
