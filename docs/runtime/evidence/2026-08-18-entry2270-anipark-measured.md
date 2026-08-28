# Port 2270 is AniPark, not ProudNet — measured from the unpacked client

**Date**: 2026-08-18
**Method**: static disassembly (Capstone x86) of the unpacked client dump
`goley-unpacked-20260816-011037.dump`
(SHA-256 `AA34F37364069652ED7CE6AAB105DD43BA7F61D7C09EC2A231B1B9E9D44D7BA1`,
ImageBase `0x00400000`, flat image so `VA - 0x400000 = file offset`),
cross-checked against live server logs from the 2026-08-18 14:32 run.

Everything below is read out of the client's own code. Nothing is inferred
from other projects, and nothing is guessed.

---

## 1. The decisive finding

`0x00A694B0`, inside the shared inbound frame validator `sub_A69440`:

```asm
00A694B0  cmp  byte ptr [eax], 0x32     ; AniPark magic
00A694B3  je   0xA694CE                 ; ok -> continue
00A694B5  mov  eax, [ebp+8]
00A694BD  mov  dword ptr [eax+edx*4], 5 ; connection error state = 5
00A694C4  or   eax, 0xFFFFFFFF          ; return -1
00A694CB  ret  8
```

A frame whose first byte is not `0x32` is **dropped silently**: error state 5,
return −1, no popup, no disconnect.

The validator is shared across connections — it indexes them by register
(`[edx + edi*4 + 0x25C]`, `[ecx + edi*8 + 0x2A0]`), so the auth socket (8000)
and the entry socket (2270) run the **same** framing and cipher.

After the magic check it validates the sequence byte:

```asm
00A694CE  mov  ecx, [ebp+8]
00A694D4  cmp  dword ptr [ecx+edx*4+0x128], 2   ; connection state == 2 ?
00A694E2  mov  dl, byte ptr [eax+3]             ; frame byte 3 = seq
00A694E5  cmp  dl, byte ptr [ecx+edi*8+0x2A0]   ; must match expected seq
```

**This explains the stall exactly.** On 2026-08-18 we sent a ProudNet
opcode-4 frame beginning `13 57 02 38 01 04`. First byte `0x13` != `0x32`
→ error state 5 → silence. The log matches perfectly: `opcode4_flush_complete`
and then nothing at all — no opcode-5, no error, no EOF.

## 2. ProudNet exists, but not on this socket

Scanning **all** executable sections for the ProudNet TCP magic `0x5713`
yields exactly two real code sites:

| VA | Instruction |
| --- | --- |
| `0x0058DF8F` | `push 0x5713` |
| `0x0058E373` | `mov edx, 0x5713` / `cmp word ptr [esp+0x54], dx` |

Both live in the `0x0058xxxx` module. The login/entry protocol lives in
`0x00A6xxxx–0x00A9xxxx`. There is **no** `0x5713` anywhere in the login code.

ProudNet *is* statically linked (RTTI present: `.?AVCNetClientImpl@Proud@@`,
`.?AVC2SProxy@CNetClientImpl@Proud@@`, `.?AVProxy@ProudC2S@@`, …), but it is
the P2P / game-relay layer. This mirrors S4 League / NetspherePirates, where
ProudNet is used only for the relay server while auth and login use the
vendor's own protocol.

**Conclusion: 2270 = AniPark. ProudNet belongs to the game layer (:20260).**

## 3. The opcode dword is scrambled

Message pump at `0x00A98A30`:

```asm
mov   ebx, dword ptr [ebx]          ; first dword of the decrypted payload
imul  ebx, ebx, 0xAAF29BF3
shr   ebx, 0x10
mov   edx, ebx
shr   edx, 8                        ; main = (v >> 8) & 0xFF
                                    ; sub  =  v       & 0xFF
```

So `v = (wire_dword * 0xAAF29BF3) >> 16`, `main = (v >> 8) & 0xFF`,
`sub = v & 0xFF`. The low 16 bits of the wire dword are free entropy (a nonce),
which is why the same logical opcode looks different on the wire each session.

To *emit* an opcode, invert it: `K⁻¹ = 0xA7F0753B` (mod 2³²), and
`wire_dword = ((main << 8 | sub) << 16) * K⁻¹ mod 2³²`.

**Verification against measured traffic:** the client's handoff frame on 2270
carried first dword `0x5046F118`. De-scrambled → `0x0000` → `main=0, sub=0`.
Hitting `0x0000` by chance is 1-in-65536, so the model is confirmed.

The auth socket uses the same scheme with a runtime multiplier from
`[0x1022C68]`; the entry socket's multiplier is hardcoded.

## 4. What the client expects in reply to `(0,0)`

Dispatcher `sub_A93360` → opcode 0 → sub-table `0xA944F4` → sub 0 →
`0x00A933CE` → `sub_A907C0`:

```asm
00A907FA  cmp  byte ptr [ebx+0x0B], al   ; al = 0  -> status byte
00A907FD  je   0xA90895                  ; == 0 -> SUCCESS
; else:
00A9082F  push 0x2715
00A90837  call 0xB209B0                  ; error popup, code = 0x20 + status
```

Success path `0x00A90895`:

```asm
00A90895  mov  edi, [esi]
00A90897  lea  eax, [ebx+0x0D]           ; key buffer at offset 0x0D
00A9089B  mov  byte ptr [esi+0x200D], 2  ; ENABLE the entry dispatcher
00A908A2  call 0xA68FA0                  ; install stream-cipher key
00A908A7  mov  eax, [ebx+4]              ; dword at offset 0x04
00A908AC  call 0xA69040
```

### Measured reply schema for `(0,0)` on 2270

| Offset | Meaning |
| --- | --- |
| `0x00..0x03` | opcode dword, must de-scramble to `main=0, sub=0` |
| `0x04..0x07` | dword consumed by `sub_A69040` |
| `0x0B` | **status byte — MUST be 0**, otherwise popup `0x20 + status` |
| `0x0D..` | stream-cipher key buffer (`sub_A68FA0`) |

`[ctx+0x200D] = 2` is exactly the flag the pump tests
(`cmp byte ptr [edi+0x200D], 0`) before routing frames to `sub_A93360`.
Until this reply arrives, the entry connection has **no dispatcher at all** —
which is why the client sat mute and eventually reported
*"Lobiye giriş başarısız"* (lobby login failed).

**Cross-check:** our port-8000 packet-1 reply, which the client already
accepts, is byte-for-byte this shape — zeros with `"RealMadrid\0"` at offset
13 (`0x0D`) and status 0 at offset 11 (`0x0B`). Disassembly and working code
agree independently.

## 5. Entry dispatcher surface

`sub_A93360` prologue: `cmp ecx, 0xD2` → index table `0xA94420` (211 bytes)
→ jump table `0xA94364`; 47 distinct cases, default `0xA94343`.

| handled opcodes (45) | 0x00, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0xC8, 0xD2 |

Sub-tables: opcode `0x00` → `0xA944F4` (sub 0..0x0C, sub 7 unhandled);
opcode `0x05` → `0xA94528` (sub 0..6, sub 2 unhandled).
Opcode `0x00` sub `0x02` reads a reason byte at `[msg+9]` and then closes the
socket (`sub_A690B0`) — that is the server-initiated disconnect path.

## 6. Code change

`crates/server/src/entry.rs`: the 2270 handler no longer speaks ProudNet. It
now parses AniPark `0x32` frames, de-scrambles and logs every opcode, and
answers the `(0,0)` handoff with the measured cipher-setup frame. It sends
**nothing else** — every further reply must be measured first.

`handle_proudnet_connection` is retained unchanged for the game layer (:20260).

## 7. Corrections to earlier evidence

`2026-08-17-auth8000-response.md` §3 "Exact Resolution" proposed a three-frame
AniPark reply on 2270 (session confirmation `0x38790000`, channel list
`0x8F000000` with 22 channels). Those opcode dwords and payloads were **not**
measured and do not survive the scramble analysis above. Treat that section as
withdrawn; the protocol family it assumed (AniPark on 2270) was right, the
bytes were not.

## 8. Open questions (next measurements)

- The dword at reply offset `0x04` (`sub_A69040`) — semantics unknown.
- Which opcode the client sends immediately after the dispatcher is enabled.
- Whether the entry cipher key must differ from the auth key.

---

# Addendum — second pass (same day)

After the corrected `(0,0)` reply the client accepted the frame but still sent
nothing. `sub_A907C0` returns without transmitting, so the client is **waiting
for the server to push**. Further measurements:

## 9. The auth channel uses the same scramble, with a different multiplier

`[0x01022C68] = 0xD9820375` (the runtime multiplier used by the auth
dispatcher; the entry channel's `0xAAF29BF3` is hardcoded).

Decoding known-good auth traffic with it:

| frame | wire dword | → (main, sub) |
| --- | --- | --- |
| client packet 1 | `0x00000000` | (0, 0) |
| client packet 2 (credentials) | `0xC9D1FF86` | **(11, 4)** |
| our packet-2 reply | `0x7F000000` | **(11, 0)** |

Small, clean opcodes on a channel that demonstrably works — the scramble model
is confirmed on both channels.

## 10. Auth packet-2 reply schema (measured in `sub_A749E0`)

```asm
00A74A2B  mov   eax, [ebx+0x1A]          ; entry server IP   (u32)
00A74A34  movzx eax, word ptr [ebx+0x16] ; entry server PORT (u16)
00A74C03  mov   byte ptr [edi+0x200D], 1 ; entry conn state := 1
00A74C0A  mov   ecx, [eax+0x0F]  -> [edi+0x2104]
00A74C13  mov   edx, [eax+0x04]  -> [edi+0x2108]
00A74C1C  mov   eax, [eax+0x08]  -> [edi+0x210C]
00A74C29  call  sub_A7EB10                ; send handoff on 2270
00A74C31  call  sub_A95180(ctx, 2)        ; app state := 2
```

| offset | meaning |
| --- | --- |
| `0x00` | opcode dword → (11, 0) |
| `0x04` | token → `[ctx+0x2108]` |
| `0x08` | token → `[ctx+0x210C]` |
| `0x0F` | token → `[ctx+0x2104]` |
| `0x16` | entry port (u16) |
| `0x1A` | entry IP (u32) |

If both IP and port are zero the client raises popup `(0x2715, 0x26)`.
Our existing `auth.rs` reply already uses exactly these offsets — that part was
correct and stays.

## 11. Entry connection state byte `[ctx+0x200D]`

`0` → `1` (handoff sent, `0xA74C03`) → `2` (our `(0,0)` reply, `0xA9089B`).
The pump only routes frames to `sub_A93360` once this byte is non-zero, so the
`(0,0)` reply is what brings the entry connection to life.

## 12. Why the previous three-frame attempt failed

De-scrambling the invented dwords with the measured entry multiplier:

| invented dword | → (main, sub) | fate |
| --- | --- | --- |
| `0x38790000` | (221, 219) | main > `0xD2` → `ja default`, **discarded** |
| `0x8F000000` | (189, 0) | not in the handled set → **discarded** |
| `0xA80B0000` | (43, 113) | not in the handled set → **discarded** |

All three were silently dropped before reaching any handler. The *choice* of
`sub_A90D00` as "session confirmation" was right; the opcode encoding was not.

## 13. Next server→client message: `(0, 0x0B)` → `sub_A90D00`

Correct wire dword: **`0x09890000`** (verified: `descramble(0x09890000) = (0, 11)`;
the scramble/de-scramble pair round-trips over all 65,536 opcode pairs).

```asm
00A90D07  mov   eax, [esi+0x0A]      ; dword -> sub_A8E3F0
00A90D0A  movzx ecx, byte [esi+0x04] ; byte  -> sub_A8E3F0
00A90D0E  movzx edx, byte [esi+0x08] ; byte  -> error code
00A90D16  call  sub_A8E3F0
00A90D1B  cmp   byte ptr [esi+0x08], 0
00A90D1F  je    done                 ; 0 -> keep socket open (success)
00A90D23  call  sub_A690B0           ; non-zero -> CLOSE the socket
```

| offset | meaning | status |
| --- | --- | --- |
| `0x00` | opcode dword `0x09890000` | measured |
| `0x04` | byte → `sub_A8E3F0` | **unmeasured**, sent as 0 |
| `0x08` | error code — **must be 0** | measured |
| `0x0A` | dword → `sub_A8E3F0` | **unmeasured**, sent as 0 |

`entry.rs` now sends this frame immediately after the `(0,0)` setup. The two
unmeasured fields are left zero deliberately — neutral defaults, not invented
values. The client's reaction to them is the next measurement.

---

# Addendum 2 — transport proven, login message identified

## 14. Disconnect probe: the transport layer is solved

Sent `(0, 0x02)` (wire dword `0xEA760000`), whose handler at `0x00A933E8`
unconditionally calls `sub_A690B0`:

```
15:16:29.344  entry_setup_sent              seq=0
15:16:29.344  entry_disconnect_probe_sent   seq=1  opcode_dword=0xEA760000
15:16:29.845  entry_client_eof              client closed the 2270 connection
```

The client closed the socket ~0.5 s later and raised Hata 93. To reach that
handler the frame had to pass magic, length, the **sequence check**, and
`sub_A6A650` **decryption**, then be de-scrambled to `(0, 2)` and dispatched.

**Therefore: framing, dummy-byte handling, the stream cipher, sequence numbers
and the opcode scramble are all correct.** Everything remaining is content.

## 15. Opcode → UI screen map

Cross-referencing each dispatcher handler with the UI element names it pushes:

| opcode | screens touched |
| --- | --- |
| `0x00` | **IntroPane**, CardBookPane, EditCardPane, MatchingPopup |
| `0x03` | MessengerPane, UINavigationPane (sub `0x08` → GameRoomPane) |
| `0x04` | GameRoomPane, GameRoomChatPane, LoadingPopup |
| `0x07` | LobbyPane, MatchingPopup, MultiLeagueJoinPopup |
| `0x0C`/`0x0D` | IntroPane, LoadingPopup, EditCardPane |
| `0x0E`,`0x18`,`0x1D`,`0x1E` | MultiLeagueLobbyPane |
| `0x66`,`0x68` | GameRoomPane, LobbyPane |

The login/intro flow lives under main opcode `0x00`.

## 16. `(0, 0x0B)` is a notice, not the login result

`sub_A8E3F0` builds an 8-byte struct `{u8 status, u8 b4, u16 pad, u32 dw}` and
compares it against the stored copy at `[[0x12BC9C4]+0x2C]+0x3830`. If they are
**identical it returns immediately** (`0x00A8E464 → 0xA8E868`). Our all-zero
payload matched the zeroed initial state, so it was a no-op *by design*. On
change it stores the new value and formats localized string `0x2A95` with the
dword — i.e. an announcement/kick notice.

## 17. `(0, 0x01)` IS the entry login result — `sub_A90670`

```asm
00A906B0  cmp   byte ptr [esi+0x0F], 0   ; status byte
00A906B3  je    0xA90743                 ; 0 -> SUCCESS
00A906CD  movzx edx, byte ptr [esi+0x0F]
00A906DA  add   edx, 0xA0                ; popup id = 0xA0 + status
00A906E9  call  sub_B209B0
; success path:
00A9076C  lea   edx, [esi+4]             ; cipher key buffer
00A90770  mov   byte ptr [ebx+0x200D], 2 ; entry connection live
00A90777  call  sub_A68FA0
00A9077C  mov   eax, [esi+0x12]          ; dword
00A90781  call  sub_A69040
00A90788  call  sub_A7DC00               ; follow-up 1
00A9078F  call  sub_A7DF60               ; follow-up 2
00A90795  call  sub_A7E5B0               ; follow-up 3
```

| offset | meaning | status |
| --- | --- | --- |
| `0x00` | opcode dword → (0, 1) | measured |
| `0x04` | cipher key buffer (NUL-terminated) | measured |
| `0x0F` | status — 0 = success, else popup `0xA0+status` | measured |
| `0x12` | dword → `sub_A69040` | **unmeasured**, sent as 0 |

The key at `0x04` is NUL-terminated, so `"RealMadrid\0"` occupies `0x04..0x0E`
and the status byte lands exactly at `0x0F` — a self-consistent layout.

The three follow-up calls are the first client-side actions after login, so
this is the message expected to make the client start sending requests.

`entry.rs` now sends `(0,0)` cipher-setup followed by `(0,0x01)` login-OK.

## 18. Bonus: `(3, 0x08)` is a server redirect

`sub_A8F580` reads bytes `0x09..0x0C` as an IPv4 address (formatted via the
`%d.%d.%d.%d` template at `0xFA3200`), a `u16` at `0x04`, and builds
`GameRoomPane`, then advances the app state to 3 via `sub_A95180(ctx, 3)`.
That is the "go to game server IP:port" message — useful later, not now.

---

# Addendum 3 — the client talks; the create-team screen located

## 19. Login confirmed live

After `(0, 0x01)` LOGIN-OK the client executed its three follow-up calls and
sent three real requests, unprompted:

| client_seq | opcode | payload |
| --- | --- | --- |
| 1 | `(3, 4)` | 16 bytes (opcode + uninitialised stack) |
| 2 | `(0x6A, 0)` | 16 bytes |
| 3 | `(5, 3)` | 256 bytes |

The payload tails are uninitialised stack memory, so these are parameterless
requests. The auth + entry handshake is complete.

## 20. Reply schemas for the three requests

| request | handler | payload layout |
| --- | --- | --- |
| `(3, 4)` | `sub_A76840` | byte at `0x08`, buffer from `0x09` |
| `(0x6A, 0)` | `sub_A80650` | bytes at `0x09`, `0x0A`, buffer from `0x0B` |
| `(5, 3)` | `sub_A86B60` | `lea esi,[ecx+5]; mov ecx,0xB0; rep movsd` → **704 bytes at offset `0x05`** |

`sub_A86B60` then looks up `UINavigationPane` and `MessengerPane` and refreshes
them — it is a data update, not a scene change.

All three were answered; the client accepted them (no popup, no disconnect) but
stayed on the login screen, confirming none of them drives the transition.

## 21. Variable-size framing

Payloads are no longer a fixed 28 bytes, and the dummy-byte count depends on the
declared wire length, so `wire = plain + dummies(wire)` is self-referential.
`auth::wire_len_for()` solves for the fixed point instead of hardcoding sizes.
Verified: 28 → 36 (matches the known-good frames), 709 → 734, 712 → 738.

Note there can be more than one fixed point (256 → 276 and 281 both work; the
client itself used 281). Either is self-consistent, since both sides derive the
dummy layout from the same declared wire length.

## 22. `(0, 0x09)` → `sub_A876C0` opens the create-team screen

```asm
00A876F3  mov   esi, [ebp+0xc]        ; message
00A876F9  add   esi, 8                ; player block at payload offset 0x08
00A876FC  add   edi, 0x109
00A87702  mov   ecx, 0xB0
00A87707  rep movsd                   ; 704 bytes
...
00A877B7  mov   dl, byte ptr [esi+7]  ; SCREEN SELECTOR at payload offset 0x07
00A877BA  cmp   dl, 2
00A877BD  ja    0xA87956
00A877D1  test  dl, dl
00A877D5  push  0xF8EE54              ; dl == 0 -> "IntroBIPane"
00A877F4  cmp   dl, 1
00A877F9  cmp   dl, 2
00A877FE  push  0xF8EE60              ; dl == 1 or 2 -> "CreateTeamPane"
00A87844  mov   byte ptr [ecx+8], 1   ; one-shot guard
```

| offset | meaning | status |
| --- | --- | --- |
| `0x00` | opcode dword → (0, 9) | measured |
| `0x07` | screen selector: 0 = IntroBIPane, 1/2 = **CreateTeamPane** | measured |
| `0x08` | 704-byte player block | size measured, contents **unmeasured** (zero) |

The pane switch is guarded by `[[0x12BC9C4]+0x54]+8 == 0` and sets that byte to
1 afterwards, so it fires only once per session.

`entry.rs` now pushes `(0, 0x09)` with selector 1 after answering `(5, 3)`.

---

# RESULT — login passed, character-creation screen reached (2026-08-18)

Sending `(0, 0x09)` with selector byte `[0x07] = 1` opened **CreateTeamPane**.
The client left the login window and rendered the character-creation flow:

> `İsim  ›  Takım Seç  ›  Desteni Aç  ›  Başlat`
> "Şimdi Gerçek Futbol Zamanı! — Takımınızı kurun ve hemen Goley oynamaya başlayın!"
> name field + availability ("Uygunluk") button, 2–12 characters.

## Full working sequence on port 2270

| # | direction | opcode | meaning |
| --- | --- | --- | --- |
| 1 | C → S | `(0, 0)` | AniPark handoff (285 bytes) |
| 2 | S → C | `(0, 0)` | cipher setup — sets `[ctx+0x200D] = 2` |
| 3 | S → C | `(0, 1)` | LOGIN-OK, status `[0x0F] = 0` |
| 4 | C → S | `(3, 4)` | post-login request 1 |
| 5 | S → C | `(3, 4)` | reply |
| 6 | C → S | `(0x6A, 0)` | post-login request 2 |
| 7 | S → C | `(0x6A, 0)` | reply |
| 8 | C → S | `(5, 3)` | player-data request |
| 9 | S → C | `(5, 3)` | 704-byte player block at offset `0x05` |
| 10 | S → C | `(0, 9)` | selector `[0x07]=1` + 704-byte block at `0x08` → **CreateTeamPane** |

Preceded by the auth gate on port 8000: `(0,0)` challenge → `(0,0)` cipher
setup → `(11,4)` credentials → `(11,0)` redirect (port `0x16`, IP `0x1A`).

Everything above is derived from the client's own code or its live traffic.
No fabricated bytes remain in the entry path.

## What is still unmeasured

- The 704-byte player block is all zeros. Its layout is the next target; one
  flag inside it is known to matter (block offset `0xA4`, read at `0x00A8770C`
  and mirrored to `[ctx+0x362C]`).
- `(3,4)` and `(0x6A,0)` reply contents (byte + buffer each) are still zero.
- Selector value `2` also maps to CreateTeamPane; values `> 2` take a separate
  path at `0xA87956`, not yet explored.
- Next expected traffic: the name-availability check from the "Uygunluk"
  button, then team selection and deck opening.
