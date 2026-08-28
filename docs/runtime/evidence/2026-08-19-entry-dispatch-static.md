# Entry Dispatcher Static Analysis — 2026-08-19

## Source
- Dump: `%USERPROFILE%\AppData\Local\Temp\goley-unpacked.dump` (22,642,688 bytes)
- SHA-256: `4EB6880FD36FAD2C73B0F4AEA34BF5A1CCF89839894151A480ACA00E34540499`
- Tools: Python 3.12 + capstone (CS_ARCH_X86, CS_MODE_32)

## Key Finding: VA vs RVA Offset
Evidence documents previously listed VA (virtual addresses = image_base + RVA).
In the flat dump, file offset = RVA. **Subtract 0x400000** from all previously documented addresses.
Example: evidence `0x00A93360` → dump offset `0x00693360`.

## Dispatcher Structure

### Entry Point
`sub_A93360` (RVA 0x693360, VA 0x00A93360) — receives:
- `cl` = opcode_main (from `movzx ecx, cl`)
- `al` = opcode_sub (from `mov al, byte ptr [ebp+8]`)
- `edx` = connection/session object pointer (saved to `esi`)
- `[ebp+0xc]` = message data pointer (saved to `edi`)

### Dispatch Mechanism
```
cmp ecx, 0xd2            ; max opcode_main = 210
ja  return                ; default: ignore
movzx ecx, byte [ecx + 0xa94420]  ; byte indirection table (211 entries)
jmp dword [ecx*4 + 0xa94364]      ; jump table (47 entries)
```

### Active Opcodes (non-default handlers)
| Main | Handler VA | Sub-table size | Notes |
|------|-----------|----------------|-------|
| 0 | 0xA933BB | 12 subs (0-11) | Cipher/handshake |
| 1 | 0xA94343 | — | **No-op** (return) |
| 2 | 0xA9350E | — | al==3 check |
| 3 | 0xA936BD | 40 subs (0-39) | Team/game data |
| 4 | 0xA93901 | 4 subs | |
| 5 | 0xA9349B | 6 subs | Player data |
| 6 | 0xA93CDE | — | Simple handler |
| 7 | 0xA93529 | 9 subs | |
| 8 | 0xA935C3 | 13 subs | |
| 9 | 0xA94343 | — | **No-op** (return) |
| 10 | 0xA94343 | — | **No-op** (return) |
| 11 | 0xA9393A | 2 subs | |
| 12 | 0xA93968 | 2 subs | |
| 13 | 0xA93991 | — | Direct call |
| 100 (0x64) | 0xA9399D | 14 subs | |
| 101 (0x65) | 0xA93AA8 | 15 subs | |
| 102 (0x66) | 0xA93B89 | — | |
| 103 (0x67) | 0xA93BA5 | — | |
| 104 (0x68) | 0xA93BD6 | — | |
| 105 (0x69) | 0xA93C09 | 7 subs | |
| 106 (0x6A) | 0xA93D49 | — | |
| 107 (0x6B) | 0xA93D65 | — | |
| 108 (0x6C) | 0xA93F3E | — | |
| 109 (0x6D) | 0xA94338 | — | |
| 200 (0xC8) | 0xA93CF1 | — | |
| 210 (0xD2) | 0xA93C8D | — | Special with string ref |

All other opcodes (14-99, 110-199, 201-209) → default return (no-op).

## Opcode 0 Sub-Table (Cipher/Handshake)
Entry at 0xA933BB: `cmp eax, 0xc; ja return; jmp [eax*4 + 0xa944f4]`

| Sub | Handler VA | Call target | Notes |
|-----|-----------|-------------|-------|
| 0 | 0xA933CE | sub_A907C0 | Cipher setup |
| 1 | 0xA933DB | sub_A90670 | Login processing |
| 2 | 0xA933E8 | sub_A8E8A0 | |
| 3 | 0xA93400 | sub_A750C0 | |
| 4 | 0xA93413 | sub_A75600 | |
| 5 | 0xA9341F | sub_A754A0 | |
| 6 | 0xA9342D | sub_A7F7D0 | |
| 7 | 0xA94343 | — | **No-op** |
| 8 | 0xA9343A | sub_A84D20 | |
| 9 | 0xA9344E | sub_A876C0 | **CreateTeamPane trigger** |
| 10 | 0xA9345A | — | Sets `[session+0x108]=1` if flag |
| 11 | 0xA93478 | sub_A90D00 | Calls sub_68E3F0 |

## (0,9) CreateTeamPane Processor — sub_A876C0

Full analysis of the handler that processes the CreateTeamPane server message.

### Message Layout (712 bytes total)
| Offset | Size | Field |
|--------|------|-------|
| 0-6 | 7 | Header/padding |
| 7 | 1 | **Screen selector** (0=IntroBIPane, 1=CreateTeamPane, 2=CreateTeamPane+flag) |
| 8-711 | 704 | **Player data block** (0xB0 dwords) |

### Processing Logic
1. Copies 704 bytes from message[8] → session_object[0x109] via `rep movsd 0xB0`
2. First switch on screen selector (cmp > 2 → skip):
   - **0**: Creates "IntroBIPane" string → `sub_860480` pane creation
   - **1 or 2**: Creates "CreateTeamPane" string → same pane creation
3. Sets `[session+0x54]->byte[8] = 1` (pane-shown flag)
4. Second switch on same selector:
   - **0**: Sets `[session+0xf4]=1`, opcode-scramble response via `sub_66A420`, clears `[ebx+0x2114]`, calls `sub_682810`
   - **2**: Sets `[session+0xf4]=2`, if selector==2 looks up "CreateTeamPane" pane, sets `[pane+0x54]=2`
   - **1**: Falls through to default — **no server response sent**

### Key Insight
When server sends selector=1 (CreateTeamPane), the client:
1. Shows the CreateTeamPane UI
2. Does NOT send any response to the server (case 1 has no handler in second switch)
3. Waits for user interaction

When server sends selector=0 (IntroBIPane), the client:
1. Shows IntroBIPane
2. Sends an opcode-scrambled response back to server
3. Server can then send more data

## (3,4) Client Handler — sub_A76840
Called when client receives (3,4) from server.
- Reads `[ebx+8]` = byte, `[ebx+9...]` = data buffer
- Calls `sub_657000` with byte + data → stores player/team data
- Does map/set operations on global state
- Sets flag `[eax+0x2114] |= 0x200`
- **Server currently sends 28-byte zero-filled response — client processes zeros as player data**

## (0x6A,0) Client Handler — sub_A80650
Called when client receives (0x6A,0) from server.
- Reads `[esi+9]` = byte, `[esi+0xa]` = byte, `[esi+0xb...]` = data
- Compares with `[session+0x35f8]`
- Constructs string from `0xf99138` ("notice"?)
- Does string comparison via `sub_002EE0`
- If match: calls `sub_8BF810` + `sub_7DEF70` (notification handler)
- **Server currently sends 28-byte zero-filled response — client may not process it correctly**

## Observed Wire Flow
```
C→S  (0,0)   cipher setup
S→C  (0,0)   cipher key response
S→C  (0,1)   LOGIN-OK
C→S  (3,4)   client request
S→C  (3,4)   echo with zeros (STUB)
C→S  (0x6A,0) client request
S→C  (0x6A,0) echo with zeros (STUB)
C→S  (5,3)   player data upload (704 bytes)
S→C  (5,3)   echo with zeros + (0,9) CreateTeamPane(1, zero_player_data)
[18s gap]
C disconnects
```

## Unknowns
1. What valid 704-byte player data block contains for a new player
2. What (3,4) response should contain (not just echo with zeros)
3. What (0x6A,0) response should contain
4. Whether the client expects additional S2C messages after CreateTeamPane
5. What causes the 18-second disconnect — user timeout or protocol error
