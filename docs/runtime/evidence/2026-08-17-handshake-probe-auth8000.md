# ProudNet Handshake Probe & Port 8000 Auth Gate Live Evidence

- Date: 2026-08-17 TRT
- Target: Goley TR (`BinaryTr.bin`, SHA-256 `C136E751905FBE60CF27A71D7D05FBDE7C0428484D577F3A0A56766C839EFCFA`)
- Emulator Build: `goley-server` (`v0.1.0`), `goley-shim` with `netredirect`

## 1. Executive Summary

During live execution of the ProudNet handshake probe (Variant A / Variant B), pressing "Giriş" on the login screen produced a live measurement revealing that client authentication operates as a **two-tier network architecture**:

1. **Tier 1 — Joygame / AniPark Auth Gate (`213.74.179.12:8000` TCP)**:
   - When credentials are submitted via the in-game UI, the client connects to port `8000` via Winsock `WSAConnect`.
   - The protocol uses a proprietary AniPark framing (`0x32` magic byte) with a rotating-key XOR cipher (`sub_A6AA00` / `sub_A6A5E0`).
   - First live packet captured: 28 bytes (`32 18 00 00 ad 9a 9e 93 ...`).

2. **Tier 2 — ProudNet Entry Server (`213.74.179.12:2270` TCP)**:
   - ProudNet engine (`Proud::CFastSocket`, `Proud::CNetClient`, UTF-16LE strings at `0xEC2000`–`0xED5000`) is initialized and connects to port `2270` once the session is authorized.
   - Handshake sequence: S2C Opcode-4 (`NotifyServerConnectionHint`) -> C2S Opcode-5 (`NotifyCSEncryptedSessionKey`) -> S2C Opcode-6 -> C2S Opcode-7 -> S2C Opcode-10 -> RMI 1500+.

---

## 2. Live Capture Data

### Client Connection Event (`goley-shim` log)
```json
{
  "timestamp": "2026-08-17T15:07:22.223877Z",
  "level": "INFO",
  "message": "valid nonmatching IPv4 connection attempt passed through unchanged",
  "event_type": "network_connect_attempt",
  "api": "WSAConnect",
  "socket": 3224,
  "matched": false,
  "original_destination": "213.74.179.12:8000",
  "caller_module": "BinaryTr.bin",
  "caller_offset": 6723440,
  "caller_address": 10917744
}
```

### Server Inbound Raw Bytes (`goley-server` log)
When port 8000 traffic was redirected to the local probe listener:
- **Bytes Count**: 28 bytes
- **SHA-256**: `df751124727effbd832813ff94c91d6ac83030c864b6a5806502da1be5c1202e`
- **Hex Dump**:
  `32 18 00 00 ad 9a 9e 93 25 a3 b1 9e 9b 8d a6 00 24 4c 9b c9 62 75 6d 50 51 20 4c 60`

---

## 3. Disassembly & Binary Analysis (Session `3ace189f`)

### Frame Header Format (`sub_A69440` / `sub_A6A270`)
| Offset | Type | Field | Measured Value |
|---|---|---|---|
| `[0]` | `u8` | Protocol Magic | `0x32` (`50` decimal) |
| `[1..2]` | `u16` LE | Payload Length | `0x0018` (`24` bytes) |
| `[3]` | `u8` | Sequence Number | `0x00` (increments per packet) |
| `[4..27]` | `[u8; 24]` | Encrypted Body | `ad 9a 9e 93 25 a3 b1 9e 9b 8d a6 00 24 4c 9b c9 62 75 6d 50 51 20 4c 60` |

### Key Functions Identified
- **`sub_A695A0`** (`0x00A695A0`): Winsock `socket`, `WSAEventSelect`, non-blocking `WSAConnect` caller.
- **`sub_A6A270`** (`0x00A6A270`): Outbound frame builder — prepends `0x32`, length, sequence byte, encrypts via `sub_A6AA00`, and transmits via `send`.
- **`sub_A6AA00` / `sub_A6A5E0`** (`0x00A6AA00` / `0x00A6A5E0`): Custom multi-round XOR stream cipher with key permutation table.
- **`sub_A69440` / `sub_A6A8C0`** (`0x00A69440` / `0x00A6A8C0`): Inbound frame validator and decryptor.
- **`sub_A7E8F0` / `sub_A71C80`** (`0x00A7E8F0` / `0x00A71C80`): Login UI submit handler — connects to `SERVER_IP:8000` and dispatches initial credentials payload.
- **`sub_A98770` / `sub_A74C60`** (`0x00A98770` / `0x00A74C60`): Packet dispatcher for the auth gate.
- **`sub_A745D0`** (`0x00A745D0`): Transitions client to ProudNet Entry connection on port `2270`.
- **`ProudNet` Runtime** (`0x00EC2000`–`0x00ED5000`): Statically linked ProudNet implementation with `CFastSocket`, `CNetClient`, `CMessage`.

---

## 4. Next Steps
1. Emulate port 8000 Auth Gate responses (`0x32` frame codec + auth response) in `crates/server` so the client receives successful auth.
2. Observe the subsequent transition into the ProudNet port 2270 handshake to verify Opcode-4 -> Opcode-5 with live client.
