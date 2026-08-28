# bigpickle.md — Goley Reverse Project Status Report

**Project:** Goley (차구차구 / Chagu Chagu) clean-room server emulator — Rust
**Status:** Current key goal is getting the **team-creation wizard → starter cards → formation screen** flow to render correctly in the client
**Environment:** Windows x86 (win32), PowerShell 5.1
**Note:** Absolute paths are deliberately omitted from this document. Only terminal file/directory names are shown where useful. Probe scripts live in a temp working directory; evidence docs live under `docs/runtime/evidence/` in the repo.
**Git:** The repo has **no commits yet** (`master` is empty); history is readable only from log files and evidence documents.
**Genre warning:** This report is written in a deliberately theatrical register. Every byte of data, every address, every handler name, and every timeline below is real and measured; only the narrative voice is absurd. Do not let the prose fool you into trusting it more than the hex.

---

## 0. One-line summary (as required)

The client (BinaryTr.exe) is fully original; it is driven purely from the server side through the entry.rs reply chain. The (8,1) card-batch message **has been proven in live memory** to place the records the renderer needs into the `session+0x3500` RB-tree (13 of 14 player nodes were read directly). The remaining problem is **catalog (gd+0x2BC) fill timing** and missing records (GK 32206, EM/TD cards). We have not yet reproduced the conditions of the single correct render at 23:16.

### 0.1 How to read this document

If you are new to this project (welcome; the coffee is by the door), you are
not required to read all of it in order. A suggested route:

1. **Section 0.5** — the cast. You will meet everyone, including the people
   who never appear. The narrative will make sense afterwards.
2. **Section 1** — what the previous developer accomplished. This is the
   inheritance; treat it with the respect due to someone else's well-loved
   artillery.
3. **Section 2** — our additions: the wizard reply chain, the Turkish-squad
   mapping, the memory scans, and the great trim saga. This is the meat.
4. **Sections 3-6** — inventory, plan, rules, evidence. Read these when you
   need to check a fact, find a file, or remind yourself that the project is
   still bound by its own laws.
5. **Appendices A-F** — optional but recommended when the absurdity of the
   above has made you doubt your memory. They re-state the important numbers
   in deadpan, un-dramatised form.

The document is long on purpose. The technical facts it contains are few but
precious — a handful of addresses, a dozen handler names, one mapping table —
and they are wrapped in a great deal of context so that no one forgets how
hard-won they were. When you see a hex address, it is real. When you see a
metaphor, it is decorative. Both are marked clearly enough to be told apart.

### 0.2 Glossary of the everyday jargon (quick reference)

A short table for people who have not yet lived inside this project long
enough to dream in its vocabulary:

| Term | Meaning |
|------|---------|
| **entry.rs** | Our 2270-port server implementation; 2332 lines; the entire opening act of the game. |
| **AniPark** | The protocol family spoken on the auth gate and, unexpectedly, on port 2270. |
| **ProudNet** | The commercial middleware (Nettention) used on the lobby/battle layers. Not a custom protocol. |
| **cdd** | card_data_id. The number the client's renderer uses to look up a card's face and name in its own database. The whole story turns on this. |
| **RB-tree** | The `session+0x3500` red-black tree that holds the player records the formation screen reads. |
| **gd+0x2BC** | The catalog — a map in the global data that the bouncer (0xA5C5F0) consults before allowing a record into the tree. |
| **frame pattern t==2** | `[cid][season][02][cid][season][cdd]` — the exact byte dance a tree node performs; our archaeological signature in memory scans. |
| **23:16** | The time of the single correct render ever observed. A Shibboleth. A ghost. |
| **the trim** | The server-side clamp that keeps the (8,1) card batch under the size the client agrees to read (16 records, ~3.5 KB). |
| **RPM** | `ReadProcessMemory` — the telescope through which we observe the client's living memory. |
| **the doorman** | gd+0x2BC; the catalog. Decides who is permitted into the tree. Unreachable by telephone. |
| **the bouncer** | 0xA5C5F0 (`card_insert_batch`); executes the doorman's verdict with zero commentary. |
| **the Phantom** | 32206, the goalkeeper, present in four places at once and absent from all of them. |

---

## 0.5 Dramatis personae — the cast of this tragedy

For the ten people who will ever read this document, here is who is who in the absurdist theatre that follows:

| Character | True identity | Temperament |
|-----------|---------------|-------------|
| **BinaryTr.exe** | The client. A 2013 Korean football manager wrapped in Themida packing and GameGuard armor. | Morose, exacting, easily offended by 12 KB messages. |
| **the entry server** | Our 2270 listener, entry.rs incarnate, 2332 lines of diplomacy. | Unshakably patient, pathologically literal. |
| **session+0x3500** | The player-card RB-tree the renderer actually cares about. | A hoarder with excellent bookkeeping habits and a guest list. |
| **gd+0x2BC** | The catalog — the doorman. Decides who is allowed into the tree. | Extreme unprofessionalism: never answers the phone when the probe calls. |
| **0xA5C5F0** | `card_insert_batch` — the bouncer. Silent, binary-minded, merciless. | Judged every record by a single field and never apologized. |
| **32202** | TUGAY, the centre-back. cdd 102933081. The most photographed player in memory. | Reliable, present, annoyingly well-documented. |
| **32206** | OMER, the goalkeeper. cdd 102933120. The Phantom. | Present in four rooms of the house at once, yet never at the party. |
| **EM / 3001 | 1001 | 4001** | Club crest cards. Shared a face with 32202 yet were never photographed together. | The identical twin that never shows up in the group photo. |
| **TD / 1** | The technical director. Same cdd as 32202. Also invisible. | The manager's chair was empty and no one noticed. |
| **23:16** | The hour of the single correct render. A ghost in the timeline. | Appeared once, uninvited, and has never returned. |
| **the Sage** | A CPU-emulation library we use to run the client's own decrypt routines. | A medium through which we interrogate the dead (the cipher). |
| **the Pirate** | An unlicensed behavioral specification; read-only, never copied. See §7. | Legendary, unhelpful when directly summoned, invaluable as rumor. |
| **the probe scripts** | Python skulls that poke the dead via `ReadProcessMemory`. | Overly chatty; one of them lied to us for three days. |
| **the 12 KB envelope** | The original untrimmed (8,1) batch. The letter the client refused to open. | Slightly too heavy for the client's mailbox; cause of the first failed engagement. |
| **the doorman's booth** | gd+0x2BC viewed as a place, with a chair and a phone that never rings. | Permanently staffed, permanently unreachable. |

### 0.5.1 A note on the stage directions

This section is called "dramatis personae" but it functions as a *briefing*.
Every character above corresponds to something real — an address, a buffer, a
handler, an observed behavior. The personification is a mnemonic, not a
delusion. When we say "the bouncer turned the goalkeeper away," we mean
`card_insert_batch` returned without inserting because the catalog lookup for
cdd 102933120 failed or was never performed. When we say "the client walked
out," we mean the client closed its socket roughly half a second after
receiving the 12 KB batch we sent. The metaphors are load-bearing only in the
sense that they let you hold the whole sad story in your head at once. The
byte offsets remain the final authority at all times.

---

## 1. What the previous developer did

Work that was already in place when we took over, listed with its evidence. Imagine dusty artillery that had already been aimed: we only had to fire it (and, occasionally, apologise to it).

### 1.1 Workspace and clean-room architecture

- Multi-crate workspace with a single-direction dependency chain:
  `proudnet → glyproto → domain → server`. `proudnet` depends on no other
  workspace crate (standalone publish goal). It is the hermit of the family;
  it refuses to speak to its cousins, by design, so that it may be published
  alone one day, like a successful lighthouse keeper.
- `.gly` schemas are the single source of truth for the protocol; packet
  structs are never hand-written. The schema is the constitution; the Rust
  code is merely the obedient civil service. This was decided for a reason
  that still holds: in a clean-room project there is no production binary to
  *read*, so the protocol's definition is maintained as data, reviewed as
  text, and generated into code — so that a disagreement between the schema
  and the code is impossible by construction.
- Hard rules in `AGENTS.md`: never commit client files/assets/dumps; never copy
  code from "the Pirate" (read-only behavioral specification; see the
  References section); model unknown fields as `unknown`, never silently
  discard them. Rule zero: ignorance is a feature request, not a defect. An
  unknown field is a debt we acknowledge in writing; a silently-dropped byte
  is a debt we pretend doesn't exist. One is solvent, the other is fraud.
- Crates: `proudnet`, `glyproto`, `glyproto-schema`, `volante`, `domain`,
  `server`, `patchd`, `persistence`, `trace`, `conformance`, `goley-boot`,
  `goley-shim`; tools `gly-extract`, `gly-cov`. A small army that respects
  one-way streets. `conformance` exists to test that the codec the schema
  generates is the codec the wire actually speaks; `trace` and `persistence`
  are the project's long-term memory; `patchd` is the asset-patch server that
  serves the layered updates the client expects. Not everything in this army
  is currently on the front lines, but the chain of command is unambiguous.
- Licensed MIT/Apache-2.0; reference projects are credited under the nicknames
  listed in the References section. The workspace is intended to be
  redistributable; the licenses are the paperwork that makes that claim true.

### 1.2 goley-boot / goley-shim — run & observation tooling

- `goley-boot` (32-bit, `requireAdministrator`): `CreateProcessW(CREATE_SUSPENDED)`,
  DLL injection into the client, two-stage `LOADED` → `READY` event handshake.
  It is the midwife: the client is born, suspended before its first breath,
  inspected, and only then allowed to inhale. The two-stage handshake is
  there because the client is packed and must be given time to unpack before
  anyone speaks to it; `LOADED` means the DLL is in the process, `READY` means
  the client's own unpacking has reached the point where our code is safe to
  run. The distinction is not ceremony; it is the difference between arriving
  at a party and being *fashionably* arrived.
- `goley-shim` (32-bit DLL): Themida unpack-readiness polling (never writes
  software breakpoints), observes named kernel objects (CreateEvent/Mutex/Wait
  hooks), `ExitProcess`/`TerminateProcess`/`NtTerminateProcess` exit guard, and
  a `netredirect` feature that rewrites only the measured
  `213.74.179.12:2270` route to an explicitly configured local listener
  (`ws2_32!connect` / `WSAConnect`). A polite burglar: it only reroutes one
  street, spares the rest of town. It deliberately does not block traffic to
  any other endpoint — the GameGuard and patch infrastructure are left to
  behave as they always have, because lying about more than one address at a
  time is how you lose track of which lie is doing the work.
- Static patch policy: patch data lives only in `patches.toml` as `{rva,
  original_bytes, patched_bytes, note, build_sha256}`; nothing is written
  without full SHA-256 + original-byte verification. The client's body is
  sacred; every edit must be notarised in triplicate. The SHA-256 column is
  what lets the same project keep two different client builds (2015-09-22 and
  2016-03-18) in the world side by side: each patch is keyed to the exact
  build it was measured against, and a mismatch is treated as a stop sign, not
  a shrug.
- Docs: `boot.md` (runtime boot chain), shim/boot READMEs. The boot chain
  document is the one that explains, in order: how the process is created
  suspended, how the DLL is injected, how the two events are signalled, how
  the unpack-wait works, and how `netredirect` is armed before the client's
  networking code ever wakes up.

### 1.3 GameGuard blocker and measured patches (2026-08-16)

- The client terminated itself because the GameGuard kernel driver could not
  load. Imagine a nightclub that slams its own doors shut and then calls the
  fire department on itself because the bouncer is on a smoking break. The
  mechanism is mundane: GameGuard's kernel component refuses to start on
  modern Windows (unsigned legacy driver blocked by DSE), and the client's
  anti-cheat integration treats that failure as fatal. The sophistication was
  never in the failure; it was in the recovery.
- Two measured patches (`build_sha256 = C136E751...`) reached the login screen:
  - `rva 0x009374DB`: `cmp esi,0x755` → `cmp esi,0x17C` (accept GameGuard
    status 380 = updater-unavailable at the login gate). We did not bribe the
    guard; we simply showed the guard a document with the "correct" trivially
    wrong number on it, and the guard shrugged and waved us through. The
    status 380 is not "clean" in the GameGuard sense; 380 means the updater
    was unavailable, which is close enough to a functioning-but-quiet
    anti-cheat for the client's login gate to accept the game as legitimate.
  - `rva 0x0093BB67`: report the first failing periodic GameGuard poll's
    `0x262` return as "success" for the Error99 consumer. A small white lie,
    told to a robot, so that the human could type a Turkish username. The
    0x262 value is the anti-cheat's way of saying "I have a problem"; our
    patch makes the in-game consumer hear "all is well", which satisfies the
    Error99 dialog that would otherwise appear.
- Evidence: `2026-08-16-login-reached.md` — the Turkish username/password
  screen was reached; then `Gameguard Error99(0)` appeared. Victory was
  declared at the exact moment the client complained politely about something
  else. The Error99(0) dialog arriving at the login screen was treated as a
  milestone because it proved the client no longer died *before* the screen;
  it had simply moved its death a step later. Progress, in this project, is
  frequently measured by how far into the game the client manages to complain.

### 1.4 Client self-termination blocker analysis (2026-08-16)

- Proven that the splash does NOT wait on a GameGuard "ready event" (old theory
  disproven): `Global\MtxNPGL` returns `WAIT_OBJECT_0` immediately,
  `Global\MtxNPGM` is created, and no named ready Event exists. The client was
  not waiting for anyone; it had already decided. This is a negative result of
  real value: it removed an entire family of wrong hypotheses (that the client
  hangs awaiting game-guard synchronization) and redirected attention to the
  actual exit path.
- The client deliberately calls `ExitProcess(0)` at `BinaryTr.bin+0x4ba17d`,
  then BugTrap calls `TerminateProcess`; Windows logs fault offset
  `0x004ba17d`, exit `0x80000003`. The predicate is still unknown. We know the
  address where the client committed suicide, but not the note it left behind.
  The address is 0xf7d bytes past the raw `.text` boundary, which means it is
  only visible in the unpacked runtime (via a debugger or a full memory dump);
  it can never be seen in the static on-disk file. Whatever the client reads
  before deciding at that instruction — most plausibly a GameGuard
  initialization/status return that cannot succeed because the kernel driver
  never loaded — remains unidentified.
- Evidence: `2026-08-16-wait-handles.md`, `2026-08-16-run-blocker.md`,
  `2026-08-16-exit-stack.md`, `2026-08-16-unpacked-dump.md` (22.6 MB unpacked
  dump, SHA-256 `4EB6880...`). A full autopsy every time; the client keeps
  dying so that we keep learning.
- **Known confound, to be ruled out before blaming GameGuard:** a debugger
  was attached during the original capture. The same exit must be confirmed in
  a clean run with no debugger attached before the GameGuard blame is final.
  This is in the rules because self-fulfilling diagnoses are the occupational
  disease of investigations like this one.

### 1.5 Protocol discoveries (handshake, auth, entry)

- ProudNet handshake: the server sends opcode-4 (NotifyServerConnectionHint)
  and receives opcode-5 (NotifyCSEncryptedSessionKey); RSA-OAEP SHA-1/SHA-256
  decrypt probes live in `handle_proudnet_connection`. Two rival studies
  (SHA-1 and SHA-256) were conducted simultaneously, friends forced apart by a
  single algorithm choice. The two candidate digests were probed in parallel
  because the version of the ProudNet middleware in this 2013 build is not
  yet pinned; both variants' behaviour on the wire had to be demonstrated
  before one could be selected.
- Evidence: `2026-08-17-proudnet-hint-probe-plan.md`,
  `2026-08-17-auth8000-response.md`, `2026-08-17-handshake-probe-auth8000.md`.
- AniPark entry detection: port 2270 is **not ProudNet** — it speaks the same
  AniPark protocol as the auth gate; the inbound validator requires the first
  byte to be `0x32` and silently drops non-magic frames (`0x00A694B0`). The
  socket is a doorman who only lets in people wearing a single agreed colour;
  everyone else vanishes without so much as a "goodbye". This was a genuinely
  surprising find — the original architecture assumed the entry layer might be
  ProudNet, and the measured traffic on 2270 proved otherwise. The `0x32`
  magic requirement is the sharp edge: frames that do not begin with it are
  not answered, not logged as errors, not even acknowledged. They simply become
  nothing.
- Opcode scramble measurement: `imul ebx, ebx, 0xAAF29BF3` @ `0x00A98A32`;
  modular inverse `0xA7F0753B`; `scramble_opcode`/`descramble_opcode` are in
  entry.rs. The opcode is a password that is scrambled by a constant so
  arbitrary it looks like the author hit the keyboard with their forehead.
  That the inverse exists and is deterministic is what makes the whole scheme
  treatable; without the inverse, the server could never tell which C2S
  message the client actually wanted.

Further transport facts that frame everything below (all documented in
`AGENTS.md`, not our measurement): the ProudNet TCP frame begins with a u16 LE
magic `0x5713`, a scalar prefix byte (1|2|4), then the payload length in the
matching width; there is no frame-level checksum. Scalars write one byte under
128, two under 32768, else four. Strings carry a type byte (1 = ANSI cp1252,
2 = UTF-16LE) plus a scalar character count. Core messages are opcode-1
(Rmi), opcode-2 (UserMessage), and the encrypted/reliable/compressed variants
36/37/38. RMI parameters are encoded per the ProudNet "PIDL" style — i32/i64
LE, bool as one byte, and strings as a u32 char-count followed by UTF-16LE
without surrogate pairs. None of that is a distraction; it is the floor plan
of the house our work is set in.

### 1.6 Static analysis — dispatcher and card handlers

- Entry dispatcher `sub_A93360` (VA 0x00A93360 / RVA 0x693360): 211-entry byte
  indirection table + 47-entry jump table. `main=1` is the no-op sentinel (the
  old "opcodes 14-99 are no-op" claim was wrong). `sub_A876C0` = (0,9)
  CreateTeamPane; selector 1 → no reply expected, waits for user input. The
  dispatcher is a switchboard operator from the 1950s: 211 labelled cables, 47
  actual lines, and one cable that is labelled "do not plug in". The earlier
  belief that a large slab of opcodes were no-ops turned out to be a
  misreading of the indirection table; the table forwards far more messages
  than the naive reading suggested, and the no-op set is actually the four
  explicitly documented handlers (15,7)/(18,0)/(19,0)/(21,0).
- Evidence: `2026-08-19-entry-dispatch-static.md`.
- All card handlers are listed **ACTIVE**: (22,7)→`sub_A75A60`,
  (27,0)→`sub_A75340`, (107,0)→`sub_A867C0`, (105,0)→`sub_A892F0`,
  (103,10)→`sub_A7FFC0`, (17,0)→`sub_A76AC0`, (0,5)→`sub_A754A0`;
  (15,7)/(18,0)/(19,0)/(21,0) are no-ops. A staff of seven hard-working
  employees and four who stand at their desks doing nothing at all, paid the
  same salary. "Active" here means the client's handlers for these messages
  are not placeholders; they parse the body and mutate state. That is the
  difference between talking to a real reception desk and talking to a wall
  plaque that says "reception."
- Evidence: `2026-08-24-card-handlers-static.md`.
- Volante/VLD asset cipher notes: TEA delta + Blowfish S-box, 16-byte blocks,
  zlib; key derivation @0x4194c0, block decrypt @0x4185f0. Approach adopted:
  run the original routines under "the Sage" (a CPU-emulation library, see the
  References section) rather than reimplementing (the cipher is not
  rewritten). We treat the cipher as a musician treats a priceless violin:
  we do not build a better violin; we hire the original violinist, forever.
  The master seed is the MD5 of "VolanteEncryptKey_84106141"; the client's own
  routines (key derivation at 0x4194c0, 16-byte block decryption at 0x4185f0,
  image base 0x400000) are run under emulation exactly as shipped. Base
  archives are byte-identical between the 2015-09-22 and 2016-03-18 builds;
  updates ship as .VLP patches. The base game, we discovered, has not changed
  its handwriting in six months; all red ink came in patch layers.

### 1.7 entry.rs first flow (what existed when we took over)

- `handle_connection`: 1000ms peek; if `0x32`, AniPark; otherwise ProudNet.
  A one-second staring contest decides the entire fate of the connection.
  The peek is deliberately generous: some middleboxes and client starts are
  slow to speak, and one second of patience at the door is cheaper than a
  lifetime of misrouted sessions.
- AniPark flow: (0,0) cipher-setup reply (`"RealMadrid"` cipher key) → (0,1)
  LOGIN-OK (`status=0`, opens `[ctx+0x200D]=2`) → (0,3) channel/server feature
  init. The three were emitted once under a `setup_sent` flag. The cipher key
  is the word "RealMadrid" — a football club name for a football game's
  cryptography. Who says the developers had no sense of humour. The flag
  exists because these three messages must be sent exactly once per session;
  repeating them would confuse a client that has already moved on to the
  login dialog.
- `GOLEY_PROBE_DISCONNECT=1` diagnostic (0,0x02) forced-disconnect probe (a
  theory-proving tool, not a fix). A surgical self-imposed wound used only to
  test whether the corpse is still breathing.
- Reply philosophy: never invent frames; every reply derives from the client's
  own code or live traffic (unknown opcodes are only logged in the `_ =>` arm).
  We are archivists, not authors. When we do not know, we take notes; we never
  improvise a sonnet at a funeral. This is the single most important working
  rule of the project, and it predates us: the client is the oracle, and the
  server's job is to be its echo, its stagehand, and occasionally its
  magician's assistant — never its ventriloquist.

---

## 2. Our work — from when we took over the project

### 2.1 Handover point

- The client reaches the login screen (with the GameGuard "Error99(0)"
  dialog alongside). A door had been opened; behind it was another door.
- The AniPark entry connection and initial reply chain worked, but the
  **post-team-creation blank/broken cards** issue was unresolved. The team was
  created; the cards refused to show their faces. The formation screen was a
  stadium in a blackout.
- (3,4) and (0x6A,0) replies were "echo with zeros" stubs; card/pack handler
  replies did not exist yet (evidence docs mark them "disassemble next"). We
  inherited a house with several rooms that echoed back whatever you shouted.
  The stubs were not wrong in a crashing sense; they were wrong in a subtler
  one — the client nodded politely at the zeros and then kept waiting for
  content that never came, which is how a politely-nodding client becomes a
  blank-silhouette-rendering client.

### 2.2 Team-creation wizard — full reply chain (entry.rs)

After the handover, entry.rs grew to its full 2332-line flow. Measured handlers:

| C2S | Reply | Measured handler |
|----|-------|----------------|
| (0,0)/(0,1)/(0,3) | cipher-setup + LOGIN-OK + channel init | sub_A907C0 / sub_A90670 |
| (3,4) | tournament records (5×208B) | sub_A76840 |
| (5,0) | name check → (5,1); commit → (5,0) | sub_A923F0 / sub_A92590 |
| (5,1) | team-selection confirm → 5×582B starter league records, then (8,1) push | sub_A923F0 |
| (5,2) | starter-pack distribution (5×582B) | sub_A8FCF0 |
| (5,3) | player-data echo + (0,9) CreateTeamPane | sub_A86B60 / sub_A876C0 |
| (5,4) | league clubs (5×255B) | sub_A75110 |
| (5,5) | player summary (73B) | sub_A76CC0 |
| (7,0) | multi-league records (5×582B) | sub_A8A990 |
| (7,1)/(7,5) | correlation/ack | — |
| (8,1) | **card batch** (detailed below) | sub_A80580 / sub_A5C5F0 |
| (8,4)/(8,6) | echo (semantics unmeasured) | — |
| (15,7)/(18,0)/(19,0)/(21,0) | appropriate acks | no-op handlers |
| (22,7) | pack reveal — 11×28B card records | sub_A75A60 |
| (27,0) | card inventory — 11×15B slots | sub_A75340 |
| (105,0) | channel status (type 0 → skip) | sub_A892F0 |
| (107,0) | match/room setup (8 dword params) | sub_A867C0 |
| (103,10) | game-state flag | sub_A7FFC0 |
| (17,0) | social/friend status | sub_A76AC0 |
| (0,5) | P2P peer-endpoint ack | sub_A754A0 |

The rows marked "(5,0)" are worth a moment's attention: the same opcode is
used twice for two different purposes, distinguished by the client's phase.
Before commit, (5,0) is a name-availability check that evokes a (5,1) reply;
after commit, (5,0) acknowledges the team creation. The entry server must
therefore remember where in the wizard the client currently stands. This is a
reminder that the protocol is not stateless, and the state is not always
obvious — sometimes it lives in timing, sometimes in a flag, and sometimes in
a byte of our own reply that the client tucks into a context field.

Measured constants:
- Starter league/club record: **582 bytes (0x246)**, pushed to `session+0xADC`
  (sub_456880). 5 leagues: categories `[7,3,4,8,100]` = Süper Lig, G.Avrupa,
  B.Avrupa, K.Avrupa, K-Lig. Five flavours of European football, each wrapped
  in exactly 582 bytes, like identical gift boxes of different chocolates.
  The categories are the client's internal labels, not our invention; we
  observed almost all of them in `catalog_names.txt` through the probe that
  broke the mapping open.
- Card-batch record: **217 bytes (0xD9)**; the 11 pitch pairs are at
  `rec+0xED..+0x145` (11×8B), bench at `+0x145..` (5×8B + empty slots). A
  217-byte suitcase with a precise internal layout, down to the millimetre.
  The 11 pitch pairs at `rec+0xED` are what the renderer enumerates to draw
  the formation; the bench structure after `+0x145` is where the substitutes
  sit, with empty slots explicitly present rather than omitted.
- (22,7) record: 28B (7 dwords, `rep movsd ecx=7`) → `gameCtx+0x2C+0x3FF8`.
- (27,0) record: 15B stride 0x0F. Why 15? Why 0x0F? The client knows. It is
  not telling.

### 2.3 Starter cards: Turkish 2002 World Cup squad mapping

The renderer (sub_C1638A) resolves face/name from the **catalog via the
card_data_id (cdd)** — not from the record's low display bytes. (Verified live
2026-08-27: with the old Korean scheme, Korean faces/names rendered correctly
*without double-click*.) The renderer does not read the player's name from the
record envelope; it reads a tiny ID number and then goes to look up the name in
a phone book it keeps under the counter. Korean names rendered fine the whole
time — the phone book had the Korean pages. We simply had to write the Turkish
pages.

This finding deserves emphasis because it explains why no amount of editing
the display bytes in our records would ever fix the images: the renderer
ignores them for identity purposes. The low bytes are surface; the cdd is
identity. A card record carrying the right display text and the wrong cdd
renders the *wrong card's* face and *wrong card's* name, honouring the book
over the envelope every single time.

So the **fix = map each starter pid to its Turkish 2002-WC catalog cdd**; the
client pulls the right image/name from its own card DB. The mapping (copy of
`catalog_names.txt`):

```
32202→102933081 TUGAY       32203→102933091 HAKAN/Şükür
32204→102933100 YILDIRAY     32205→102933110 HASAN.Ş
32206→102933120 OMER/Çatkıç  32207→102933131 MUZZY/İzzet
32208→102933141 TAYFUR       32209→102933160 UMIT.Ö/Özat
32210→102933170 ILHAN.M      32211→102933181 ERGUN.P/Penbe
32212→102933190 ABDULLAH     32213→102933200 HAKAN/Ünsal
32214→102933220 UMIT.Ö/Davala 32215→102933230 ZAFER/Ozgültekin
```

A starting eleven from the 2002 bronze-medal team, digitised with the solemn
dignity of a forensic reconstruction. One might frame this table and hang it on
a wall. Note the elegant numbering: 32202-32215 are consecutive pids, and the
cdds are not — they are drawn from the client's own Turkish card set at
irregular intervals (102933081 through 102933230), which is itself evidence
that these entries existed in the shipped catalogue and were simply never
selected by the original Korean default lineup. The mapping is a
*restoration*, not an invention — we are telling the client to use the pages
it already had.

Additionally, in the (5,1) reply `p[0xA8]` → `session+0x362C` feeds
`sub_A60F53` → `sub_A57CB0`, which expects a **1-based array index** (1..=5),
not the raw category id: `cat_1based_index = {7→1, 3→2, 4→3, 8→4, 100→5}`,
matching `LEAGUE_CATEGORIES` order. The client does not think of the leagues by
their category numbers; it thinks of them by *position*. It is a perfectly
reasonable person who insists on being addressed by ordinal. Sending the raw
category where the index is expected would select the wrong league club set —
a class of bug that is invisible in a smoke test and fatal in a neighbour's
hands.

### 2.4 (8,1) card batch — layout, trim, hold, renderer dependency

- The renderer looks up the `rec+0xED` pairs via `sub_A5A9A0` in the
  `session+0x3500` tree; a missing record → NULL → **all 11 pitch cards render
  as blank silhouettes** (jump `0x00C167F9`). A single absent library card
  causes the whole shelf to appear empty. The renderer is the most
  catastrophically literal person we have ever met: if the reference is gone,
  the answer is nothing. It does not substitute a placeholder, does not skip
  the card, does not draw a goalkeeper's uniform where a goalkeeper's face
  should be — it draws the outline of an absence and moves on to its own
  business.
- Wire: `[scramble(8,1):4][flag:1][pad:5][count:1][N×217B]`;
  `flag=0` = normal insert (sets bits 0x2114 |= 1), `flag=1` = the
  sub_A7E500 loading path (skips the cards-loaded flag). Two doors into the
  same building: one flags "cards arrived" with a banner, the other slips in
  through the loading dock, no banner, no ceremony. The flag matters because
  the cards-loaded bit (0x2114) is consulted by the formation screen to decide
  whether the card set is ready; using the wrong door makes the client think
  the cards are *present but not loaded*, which is its own special flavour of
  wrong.
- **Order matters**: `(primary_club,13)` → `(manager 1,12)` → 14 players
  `(pid,13,cdd)`. They must survive the trim. This is not a bag of sports
  cards; it is a carefully rehearsed queue. The client will notice if the
  manager cuts in line. The club (title 13) and the manager (title 12) are not
  players; they carry their own semantic slots, and the renderer distinguishes
  them by position in the batch as well as by cdd.
- The batch was originally 12 KB and the client **never dispatched it**,
  closing ~0.5s later; ~3.6 KB is demonstrably dispatched [(5,1) at 3655 bytes
  worked]. The 12 KB envelope was treated as an unexploded ordinance; the
  client took one look at the size and simply walked out. Hence the trim
  helper defaults to **16 records** (~3.5 KB).
  `GCS_CARD_BATCH_TRIM` overrides the count, `GCS_CARD_BATCH_FULL` sends the
  entire library (fragmentation test). **Trimming to 10 dropped the players
  and rendered the pitch as blank silhouettes**; 16 is correct. We deliver
  exactly sixteen pieces of luggage: no fewer (the front desk refuses), no
  more (the front desk flees). 16 is the agreed number. 16 shall always be the
  number. The "fragmentation test" mode exists to answer a specific question:
  can the client survive being handed its entire card library in one message,
  or is some length limit at play? The answer to date is that the client is
  not a fan of enormous envelopes, and 16 records is the upper bound we have
  proven the client will read.
- `card_batch_hold_ms()` (`GCS_CARD_BATCH_HOLD_MS`) adds a pause before the
  (8,1) push so an RPM poller can snapshot the live state. A dramatic pause
  before the gift is handed over, so the photographers can capture the exact
  moment the envelope enters the client's hands. Without the hold, the poller
  races a fast server and a willing client, and the snapshot is a blur; with
  it, the tree can be inspected before, during, and after the insert, which is
  how we proved the records actually land.
- `run_server_capture.ps1` gained parameters: `-VariantB`, `-ProbeDisconnect`,
  `-CardHold`, `-CardHoldMs`, `-TrimBatch`, `-FullBatch`.
  The `start.ps1` → `start.bat` chain starts the server with
  `-CardHoldMs 60000`. A minute of silence, then the push.
- A practical note on the trim internals: trimming to fewer than the sixteen
  records discards the tail of the batch, and the tail is where the players
  live (club first, manager second, players third through sixteenth). Trimming
  to 10 therefore evicted the last six players — which is why the pitch came
  out all silhouettes. Trimming to 50 would (if the client tolerated the size)
  include extra library cards nobody asked for, which is harmless in theory
  and unproven in practice. 16 is the number that survived all experiments.

### 2.5 The 23:16 event — the single correct render

On a long-open client the **first and only correct formation render** occurred
at 23:16: only players rendered correctly (EM — club crest card — and TD —
technical director — were broken even then). This spawned the "catalog fills
after long uptime" theory. Our goal was to reproduce the 23:16 conditions in a
configurable way.

23:16 is to this project what the Yeti is to Himalayan mountaineering: one
photograph, enormous significance, total absence of reproducibility. Eleven
players stood on the pitch with their own faces — real Turkish faces — and the
only witnesses present were a club crest and a technical director, both
blurred beyond recognition. And then it never happened again. It had become a
**memory**, and we had become believers. We have since spent our nights
trying to build a machine that reaches 23:16 permanently, like an alarm clock
that guarantees dawn.

Let us be precise about what 23:16 proves and does not prove. It proves that
the client *can* render the Turkish squad — the renderer, the catalog, the
lookup, all of them are capable of the correct result. It does not prove what
filled the catalog at that hour, and it does not prove that currently-missing
records (GK 32206, EM, TD) were present at 23:16 — the EM and TD were broken
*even then*, so the 23:16 state was itself incomplete. The event is a proof of
possibility, not a recipe.

### 2.6 Experiment 2: 60-second hold + trim 16 — result

- `start.bat` with `-CardHoldMs 60000`; server log verification:
  `card_batch_prepush_hold hold_ms=60000` (12:30:35), push at 12:31:35,
  batch = 16 records (2 + 14, trim kept all, plain_len=3483). The server
  counted to sixty out loud before presenting the envelope. The two initial
  records are the club and the manager; the fourteen are the players; the
  batch's plain (pre-framing) length is 3483 bytes — comfortably inside the
  ~3.6 KB dispatch budget the client has proven willing to read.
- User report: looked at the team screen, **not fixed**. The morning after
  the great 23:16, the dawn did not come. The pitch was dark again. The
  lengths we went to (a full minute! of waiting!) had produced nothing but
  empty stands.
- **The first inspection was misleading:** the scan said "only 2 cards" — that
  was a **probe print-bug** (see 2.8). The tree was actually populated. We had
  momentarily believed the tree held two cards, and the forest was actually
  full of trees. Our instrument had a crack in its lens, and the whole
  expedition reported a drought during a monsoon. This is the cautionary
  episode of the entire project: never trust a probe's *printed summary* over
  its *raw data*. The print filter hid thirteen of the nodes; the histogram was
  correct all along.

### 2.7 Interaction warm-up test — theory disproven

- Theory: "the catalog is lazy-filled as formation cards are clicked; a long
  open client = filled catalog". The user double-clicked card details for a
  few minutes. The theory proposed that the catalog was a library that only
  shelved books when someone asked to borrow them.
- Result: it was reported that the formation was still broken; rescan showed
  the tree unchanged.
  → The lazy-fill theory is dead. Catalog fill is not time-based or
  interaction-based; it depends on some other mechanism. The doorman remains
  at his post, and the guests remain outside, and the clicking accomplished
  nothing. The theory died with full honours, buried alongside its older
  sibling, the 60-second theory. The remaining suspect is time itself, but
  time is a very slow murderer.
- The two disproofs together narrow the search usefully: if neither a warm-up
  hold nor interactive use fills the catalog, then the fill must be driven by
  the client's own asset pipeline (a load that happens once, at some
  boundary we have not yet crossed) or by a very long timer measured in hours,
  not minutes — which is consistent with the observation that the *only*
  complete render happened on a client that had been open for a very long
  time.

### 2.8 Live memory scanning (RPM) and the critical corrected reading

Scanned all committed r+w memory for the t==2 frame pattern
`[cid][season][02][cid][season][cdd]` via WinAPI `ReadProcessMemory`
(`scan_frames_all.py`). An archaeological dig with a metal detector, over a
city the size of an entire process address space. The pattern is the
signature of a tree node: card id, season, a marker of 0x02, then a repeat of
card id and season, then the card_data_id. Every node in the player tree
wears this uniform, which makes it a perfect search target across the whole
heap regardless of where the allocator placed it.

**Wrong first reading:** the script only printed detail for cdds 081/091/0; the
other cdds were in the histogram but hidden → looked like "2 cards in tree".
The histogram actually showed noise like 6881350 (54×). A moment of
collective despair, entirely self-inflicted: the scan found plenty, and the
printout, by a flaw as petty as a typo, reported the plenty as "two cards".
For a few hours we thought the tree was a desert; it was a garden that had
forgotten to print its flowers.

**Correct reading (after fixing the print filter to list all Turkish cdds):**
the session tree holds **13 of 14 player nodes**, with addresses:

```
32202  s13 102933081 @144d8160   32203  s13 102933091 @144d8af0
32204  s13 102933100 @144d8238   32205  s13 102933110 @144d8dc0
32207  s13 102933131 @144d8e08   32208  s13 102933141 @144d9168
32209  s13 102933160 @144d8820   32210  s13 102933170 @144d8bc8
32211  s13 102933181 @144d8310   32212  s13 102933190 @144d88f8
32213  s13 102933200 @144d8088   32214  s13 102933220 @144d8f70
32215  s13 102933230 @144d8c58
```

Node dump format: `[cid][seq][02][cid][seq][cdd]` — e.g. `144d8160: 00007dca
0000000d 00000002 00007dca | 0000000d 0622a259` (0x7dca=32202,
0x0622A259=102933081). The 0x144d8xxx region is the session tree. Thirteen
chairs filled, one empty. The empty one belongs to the goalkeeper.
Thirteen of fourteen is not a rounding error; it is a pattern. The tree
accepts the field players, including the two who share a cdd with the absent
EM/TD records, and declines exactly one player — and that one player is the
goalkeeper. Whatever the tree's guest list is keyed on, it is per-record in a
way that the cdd alone does not explain.

### 2.9 card_insert_batch disassembly — the real filter (0xA5C5F0)

Full disassembly:

```
gd  = [[0x12bc9c4]]+0x20          (global→gd)
key = record[8]  (cdd)            (mov ecx,[ebx+8])
it  = map::find(gd+0x2BC, key)    (call 0x476c40)
if it == [gd+0x2C0]  → RECORD SKIPPED (silent, byte [ebp-5]=0)
else → RB-insert 0x441060(session+0x3500, record)
duplicate check: season [ebx+4] vs node+0x14, card_id [ebx] vs node+0x10
```

So **catalog presence (gd+0x2BC) is the gate**; a missing cdd silently drops
the record. The bouncer at the nightclub has one question and one question
only: "Is your name on the list?" and if the computer answers "no", the
record is escorted silently out the back door — no protest recorded, no
receipt, nothing. Contradiction with observation: EM(3001)/TD(1) share cdd
102933081 with 32202, yet 32202 is in the tree and EM/TD are not — implying
102933081 is in the catalog... (open question, see 2.11). It is as if three
people presented the same ID card, and the bouncer accepted two and
turned away the other two — only it was two against two, and the winner was
the player and not the manager's chair.

Reading the disassembly carefully: the key taken from the record is the dword
at `record+8`, which is the cdd. The catalog map is looked up at `gd+0x2BC`,
and the iterator is compared against the map's end sentinel at `gd+0x2C0`. If
the iterator equals the sentinel — not found — the insertion is skipped,
marking only a local flag byte (`[ebp-5]=0`) that the caller may or may not
consult, and the record simply evaporates. If found, the record goes through
the red-black insert at 0x441060 into `session+0x3500`, subject to a
duplicate check on season and card id. The interesting wrinkle is that the
filter's *input* is the cdd, but the duplicate check it performs is on
season/card_id — meaning the guest list and the tree's own occupancy rules are
two separate conversations, and both must end in "yes."

### 2.10 Catalog analysis — all cdds exist in memory

A separate probe counts every occurrence of the 14 cdds over all committed r+w
regions: **all 14 cdds are present in memory** (4-8 matches each; first
addresses like 0x0edxxxx/0x13e5xxxx/0x144d8xxx/0x157f0xxx...). So the catalog
exists in working memory — leaving either a gd+0x2BC lookup failure for
102933120 or a scan-pattern mismatch as the explanation for 32206's absence
from the tree. Every single guest's name is, in fact, on *some* list
somewhere in the building. The doorman is looking at the wrong list, or the
right list that simply has one name crossed out in pencil: the goalkeeper's.

The count of four-to-eight occurrences per cdd across the whole committed heap
is worth digesting: the catalog is not a single table, or it is a table whose
entries are duplicated across layers (a source list, a working copy, a
display cache, an asset-pipeline structure). That the values appear at all in
`0x0edxxxx`/`0x13e5xxxx`/`0x144d8xxx`/`0x157f0xxx` regions tells us the asset
system has already been asked about these cards at some point. The mystery is
narrowly drawn: the raw values are in the process, and yet the specific tree
node for 32206 is not.

### 2.11 Current state and open questions

**Tree contents (latest scan):**
- ✅ 13 of 14 player nodes present in the session tree with correct Turkish
  cdds. Eleven of them are on the pitch; two more are on the bench; one more
  gave up the ghost somewhere between the lobby and the dressing room.
- ❌ **32206 (GK, OMER, cdd 102933120)** — 4 occurrences in the heap but no
  t==2 tree node; catalog_names lists P_4518 OMER. The Phantom Goalkeeper
  exists at four addresses simultaneously and is at none of them. A particle
  that is everywhere in the box and nowhere detectable in the detector.
- ❌ **EM (3001 / 1001 / 4001) and TD (1)** — no tree nodes (sent with cdd
  102933081, the same as 32202). The identical twin who shares DNA with the
  guest of honour but was never let past the velvet rope.

**Open questions** (each a proud, unanswered sphinx):
1. How/why does the catalog (gd+0x2BC) fill on a fresh client? The 60s hold
   did not fix it (the batch was already entering the tree); interaction does
   not fill it. → Long uptime (hours) or client-internal asset loading are the
   remaining candidates. Time is the last suspect left in the room, with the
   lights off.
2. Why do EM/TD (cdd 102933081) not enter the tree when 32202 shares the same
   cdd? Either the tree scan misses those nodes or the filter compares more
   than the cdd. Perhaps the bouncer checks more than the name: perhaps he
   checks the season, the card id, or a deeply private code in the record we
   have not yet named.
3. Why does 32206 drop out of the catalog in real time (missing GK catalog
   entry?). Goalkeepers, it seems, are not spared: 32206 is present as a name
   in the phone book but the phone book itself is missing the page.
4. A session probe read `[gd+0x2BC]=0` and `session+0x3500 root=0` — yet the
   scan shows nodes; those offsets/session-based addresses are unreliable for
   that session (probe scripts carry stale addresses). A realtor showed us a
   house that the blueprint said was empty, while eleven players were waving
   from the window. The most economical reading: the probe's base address for
   that session was wrong, so it dereferenced the wrong memory and got zeros —
   while the pattern scanner, which searches the whole heap, found the truth.
5. The 23:16 correct-render condition is still not reproduced. The Yeti
   remains unphotographed for a second time.
6. (New, born of the disassembly) What, exactly, distinguishes the cdd it
   accepted (102933081 for the players) from the cdd it accepted the players
   with, when the same cdd was rejected for EM and TD? The answer may live in
   the record fields above `record+8` — the card id and season that the
   duplicate check reads — implying the gate is not purely "cdd present in
   catalog" but "cdd present in catalog AND record satisfies additional
   conditions."

---

## 3. In-repo file inventory (names only, no paths)

| Name | Contents |
|----|-------|
| `server/src/main.rs` | Combined server: auth(8000) + entry(2270) + lobby(2271), RSA generation, `--variant a/b` |
| `server/src/entry.rs` | AniPark entry flow and card/wizard reply chain (2332 lines) |
| `server/src/auth.rs` | AniPark frame build/decrypt (`build_auth_response_frame`, `remove_dummy`, `wire_len_for`) |
| `goley-shim/patches/patches.toml` | GameGuard-380 compatibility patches + example schema |
| `run_server_capture.ps1` | Build + run + full log → `server-capture.log`; `-CardHoldMs`, `-TrimBatch`, `-FullBatch`... |
| `start.ps1` / `start.bat` | Server (60s hold) + client in one script; UAC elevation |
| `docs/runtime/boot.md` (repo) + evidence files | Boot chain and evidence logs (08-16 → 08-24) |
| `server-capture.log` | Latest session log (hold/trim/push evidence) |

Probe scripts (temp working dir, session-based addresses, some stale):
`scan_frames_all.py`, `scan_cdd_heap.py`, `dump_cardnodes.py`, `probe_gd.py`,
`dis_a5c5f0_full.py`, `check_gamectx2.py`, `dump_tree2.py`, `scan_club.py`,
`scan_depot2.py`, `catalog_names.txt`. A full-scan output capture file also
exists off-repo. Each probe is a candle; several have burned their wicks down
to nothing and now live on as room-temperature superstitions (the stale
addresses).

**Build rules (AGENTS.md):** build the server for the HOST target with plain
`cargo build -p goley-server` (**NO `--target`**; `run_server_capture.ps1` →
`cargo run` runs the host debug binary). Only goley-boot/goley-shim are 32-bit
→ `--target i686-pc-windows-msvc`. A target build that lands in the
`--target`-suffixed directory is a letter to a neighbour that will never be
delivered; the project runs only the binary the launch chain itself compiles
and runs. If the server state is ever built with `--target i686-...` in the
expectation that the stack will use it, the stack will not: the target-arch
directory is a quarantine, not a delivery bay.

---

## 4. Next steps (priority order) — the campaign plan

1. Clarify the catalog reality from the cdd-count probe result: all 14 cdds
   are in memory → the catalog is not "2-entry"; re-verify the tree-node count
   and frame ordering. First, we must believe our own eyes; we have been
   burned once by a typo in a print statement.
2. Find what populates **gd+0x2BC** with (only?) 32202/32203 — the first two
   all_player_cards entries: trace the callers of the insert into the catalog
   or check the log for an RMI that would trigger a card-DB load. Find the
   doorman's source of names. Follow the paper trail; the catalog had to be
   filled by *somebody*, and that somebody is a function with an address.
   Candidates: an asset-load RMI the client sends when it first enters the
   card section, a lazy parse of the card database triggered by a screen
   transition, or a load-once-on-a-timer that only fires on very old
   sessions.
3. If the catalog fills only over hours: keep a fresh client open ~30+ min
   without interaction, rescan the tree every few minutes, and observe when
   entries appear (23:16 reproduction). Set up a stakeout. Camp outside the
   doorman's booth with a camera and wait for him to write the first name
   down. If the 30-minute window closes with nothing, extend to hours —
   the single known-positive happened after very long uptime, and a negative
   at 30 minutes does not kill the time-based hypothesis.
4. Once the mechanism is clear, implement a permanent entry.rs fix (e.g.
   re-send (8,1) at the right time, or trigger a catalog load by answering the
   client's card-detail RMI); then handle the EM/TD card and the 32206 bug
   separately. Forge the doorman's stamp; make the catalog load instantly;
   then argue with the bouncer about the goalkeeper separately. The ticket
   must be torn in order.
5. Investigate the EM/TD invisibility with a targeted dump: scan for records
   whose cdd is 102933081 *but whose season/card-id pair differs* from the
   32202 node, and confirm whether such nodes exist with a different t value
   (the scan is currently keyed on t==2) — the 32202/32203 asymmetry (see
   §4 item 2) suggests the catalog loaded only the player subset, and the
   EM/TD absence may be a *tree-scanner rejection* rather than a *server-side
   omission*.
6. Re-run probe_gd.py against a *live* client session with correct base
   addresses, so the `gd+0x2BC` and `session+0x3500` reads can be trusted for
   the first time; the earlier zero-reading is unusable and must not be
   quoted as evidence.
7. `cargo build -p goley-server` + user test. The oracle is the human at the
   screen; the build is merely the ritual that summons them.

---

## 5. Boundary rules (release assurance) — the ten commandments

- Client files, assets, memory dumps, packed binaries, capture traces → **never
  in the repo**. The client's body stays outside our walls; we admire it
  through the window, we do not keep a lock of its hair.
- No code is copied from "the Pirate"; it is a behavioral specification only.
  Read it as a description of *behavior*, write our own *implementation*.
- MIT-licensed reference tools are credited under the nicknames listed in the
  References section.
- The protocol lives only in the `.gly` schema crate; packet structs are never
  hand-written.
- Unknown fields are modeled as `unknown`; trailing bytes are never silently
  discarded. We respect the client's secrets enough to admit we do not know
  them, and we never pretend a trailing byte meant nothing.
- No local forgery of login/auth responses: only redirection + server replies;
  the login/auth reply is never spoofed. We are tour guides, not
  ventriloquists; we point the client at real rooms, we never impersonate the
  desk clerk. The client is *redirected* to our server; it is never *lied to*
  about who it is talking to.
- This document intentionally omits absolute paths and external project names;
  the omission is a rule, not an accident of style.

---

## 6. Reference evidence documents (chronological, file names only)

| Date | Document | Content |
|------|---------|-----|
| 2026-08-16 | `2026-08-16-login-reached.md` | Login screen + GameGuard Error99(0) |
| 2026-08-16 | `2026-08-16-run-blocker.md`, `-exit-stack.md`, `-wait-handles.md`, `-unpacked-dump.md` | 0x4ba17d blocker, wait capture, dump |
| 2026-08-17 | `2026-08-17-auth8000-*.md` | Auth gate/handshake probes |
| 2026-08-18 | `2026-08-18-entry2270-anipark-measured.md` | Port 2270 = AniPark |
| 2026-08-19 | `2026-08-19-entry-dispatch-static.md` | Dispatcher tables, CreateTeamPane |
| 2026-08-24 | `2026-08-24-card-handlers-static.md` | Card handlers ACTIVE + (5,0) verification |
| ~08-27/08-28 | (memory scans, disassembly — section 2 of this report) | cdd mapping, tree proof, hold experiments |

Note: the chronicle above stops at documents; our own memory-goggled exploits
(§2.8, §2.9) are recorded only in this report and in the probe scripts'
captured output, as befits legends passed down by oral tradition.

---

## 7. References (nickname glossary)

The names of external projects/repositories are intentionally not written in
this document. They are referred to by nicknames, explained below:

| Nickname | What it refers to |
|----------|-------------------|
| **the Pirate** | An unlicensed (all-rights-reserved), ProudNet-era private-server reimplementation used only as a read-only behavioral specification; no code from it may be copied. |
| **the Seas** | The other two MIT-licensed reference implementations/toolkits for the transport protocol and the asset/RMI catalogue; read as specifications, never copied wholesale. |
| **the Beacon** | The MIT/Apache-2.0 protocol-as-data reference project whose "one source of truth + code generation" architecture our `.gly` schema approach follows. |
| **the Sage** | An open-source CPU-emulation library used to execute the client's own asset-decrypt routines instead of reimplementing the cipher. |

All are credited in the repository's NOTICE file under their real names for
license compliance; this document intentionally omits them. Do not go looking
for them by nickname; the nicknames are aliases, not addresses. The NOTICE
file is the one place where they may be found in full — the reading room where
the aliases are undone.

---

## Appendix A — The important numbers, in deadpan (no metaphors)

For when the theatre of the main text has worn you down, here are the facts
alone, in the flattest possible font:

- Session tree: `session+0x3500`; insert routine 0x441060; lookup 0x476c40.
- Catalog: `gd+0x2BC` (end sentinel at `gd+0x2C0`); global-address chain
  `[[0x12bc9c4]]+0x20`.
- Gate: 0xA5C5F0 reads `record[8]` as the cdd; missing → silent skip.
- Batch message (8,1); wire layout
  `[scramble(8,1):4][flag:1][pad:5][count:1][N×217B]`.
- Batch record 217 bytes; pitch pairs at `rec+0xED..+0x145` (11×8B); bench at
  `+0x145..`.
- League/club record 582 bytes; pushed to `session+0xADC` by sub_456880.
- (5,1) league index byte at `p[0xA8]` → `session+0x362C`; expects 1..=5.
- Renderer: sub_C1638A (catalog lookup), sub_A5A9A0 (tree lookup), jump to
  blank-render at 0x00C167F9.
- The twelve cdd values: 102933081, 102933091, 102933100, 102933110,
  102933120, 102933131, 102933141, 102933160, 102933170, 102933181,
  102933190, 102933200, 102933220, 102933230 (fourteen; 32202..32215).
- GameGuard: patch RVAs 0x009374DB and 0x0093BB67; client `build_sha256`
  starts C136E751.
- Exit predicate: `BinaryTr.bin+0x4ba17d`, exit status 0x80000003.
- Dispatcher: sub_A93360 (VA 0x00A93360 / RVA 0x693360); 211-entry indirection
  table, 47-entry jump table.
- Opcode scramble: `imul ebx, ebx, 0xAAF29BF3` @ 0x00A98A32; inverse
  0xA7F0753B.
- Batch size: 16 records, plain_len 3483 < ~3600 dispatch budget.
- Batch trim: 10 records → players dropped, pitch as silhouettes. 16 → proven.

## Appendix B — Timeline of the current phase (handover to now)

| When | What happened | Verdict |
|------|---------------|---------|
| handover | Client reaches login; card screen broken; (3,4)/(0x6A,0) are zero-echo stubs. | Starting point |
| 08-27 | Korean faces render without double-click → cdd is the identity key. | Breakthrough |
| 08-27 | `catalog_names.txt` mapping built: pids 32202-32215 → Turkish 2002 cdds. | Fix defined |
| 08-27 | (8,1) trimmed from 12 KB to 16 records; client dispatches; batch lands in tree. | Working path |
| 08-27 | Trim to 10 → pitch all silhouettes; 16 restored. | Size law learned |
| 08-28 | Experiment: `-CardHoldMs 60000`; hold verified in log; screen still broken. | Negative |
| 08-28 | Interaction warm-up (clicking card details) → still broken; tree unchanged. | Lazy-fill theory dead |
| 08-28 | Wrong print filter → "2 cards" scare; filter fixed → 13 of 14 nodes visible. | Drill corrected |
| 08-28 | 0xA5C5F0 disassembled: catalog (gd+0x2BC) is the insert gate. | Mechanism bounded |
| 08-28 | All 14 cdds found 4-8× each in committed heap. | Catalog present; gate is the puzzle |

## Appendix C — The known-service registry (what the server already answers)

The full measured roster of entry-layer C2S messages and the replies we
serve, without the theatrical wrappers — for looking up any handler in a
hurry:

- (0,0)/(0,1)/(0,3): cipher setup, LOGIN-OK, channel feature init — via
  sub_A907C0/sub_A90670.
- (3,4): tournament records, 5×208B — sub_A76840.
- (5,0): name check / commit acknowledgement — sub_A923F0/sub_A92590.
- (5,1): team selection confirm; 5×582B league records then (8,1) push —
  sub_A923F0.
- (5,2): starter packs, 5×582B — sub_A8FCF0.
- (5,3): player data echo + CreateTeamPane — sub_A86B60/sub_A876C0.
- (5,4): league clubs, 5×255B — sub_A75110.
- (5,5): player summary, 73B — sub_A76CC0.
- (7,0): multi-league records, 5×582B — sub_A8A990.
- (7,1)/(7,5): correlation/ack — unmeasured.
- (8,1): the card batch — sub_A80580/sub_A5C5F0.
- (8,4)/(8,6): echo; semantics unmeasured.
- (15,7)/(18,0)/(19,0)/(21,0): acks — no-op handlers.
- (22,7): pack reveal, 11×28B — sub_A75A60.
- (27,0): inventory, 11×15B — sub_A75340.
- (103,10): game-state flag — sub_A7FFC0.
- (105,0): channel status — sub_A892F0.
- (107,0): match/room setup, 8 dword params — sub_A867C0.
- (17,0): social/friend — sub_A76AC0.
- (0,5): P2P peer-endpoint — sub_A754A0.

## Appendix D — The theories ledger (scoreboard)

| Theory | Falsified? | By what evidence |
|--------|-----------|------------------|
| "The splash hangs waiting for GameGuard readiness" | Yes | Mutexes return immediately; no ready event exists. |
| "12 KB batch is fine; just send everything" | Yes | Client refused to dispatch; walked out ~0.5s later. |
| "Trim to 10 keeps the important records" | Yes | Players live in the tail; silhouettes everywhere. |
| "A 60-second hold warms the catalog" | Yes | Hold verified in log; screen still broken. |
| "The catalog lazy-fills on card clicks" | Yes | Clicks changed nothing; tree unchanged. |
| "Only two cards are in the tree" | Yes | Print-filter bug; thirteen nodes were present. |
| "All 14 cdds are absent from memory" | Yes | All present 4-8× each across the heap. |
| "The catalog is the only gate" | Partially | It is a gate, but EM/TD share a cdd with an accepted player. |
| "Time (hours) fills the catalog" | Not yet | 23:16 is the one suspicious positive; 30-min stakeout pending. |

## Appendix E — The full diary of the two decisive experiments

For the record, the two experiments that most shaped the current
understanding, written out so that their anatomy is obvious — not just their
verdicts.

### Experiment A — the 60-second hold (2026-08-28, ~12:30-12:32)

- **Setup:** `start.bat` with `-CardHoldMs 60000`. The server was instructed
  to count a full minute of silence after emitting the (5,1) wizard replies
  and before pushing the (8,1) card batch, specifically so a memory poller
  could observe the client in a stable, pre-batch, fresh-session state and
  then again in the post-batch state.
- **What the log proved:** `card_batch_prepush_hold hold_ms=60000` appeared
  at 12:30:35; the push followed at 12:31:35; the batch contained 16 records
  (2 seed records + 14 players, trim preserved the whole list, plain length
  3483 bytes). From the server's side, the sequence was exactly as designed.
- **What the screen showed:** the formation still rendered as silhouettes when
  inspected after the hold. The user was able to navigate to the team screen
  and the cards did not gain their faces.
- **What the memory scan then revealed:** the tree was, in fact, populated
  with the 13 player nodes listed in §2.8. The appearance of emptiness on the
  screen and the reality of fullness in the tree existed at the same time.
- **Conclusion:** the hold did not fail to deliver records — the records were
  delivered and stocked. The failure is downstream: the records exist in the
  tree but the renderer either cannot see them (a different tree?), or sees
  them but draws from a catalog that is still not filled.

### Experiment B — the interaction warm-up (2026-08-28, later)

- **Setup:** a fresh client, same 16-record batch path. The hypothesis: the
  catalog only shelves entries after the player actually interacts with a
  card's detail view, so a "warmed" client would render correctly. The human
  operator double-clicked several card detail views for a few minutes.
- **What happened:** the formation remained visually broken throughout and
  after the clicking.
- **What the scans showed:** a rescan of the tree after the interaction
  produced an unchanged node set. Nothing new entered the tree as a result of
  the clicks.
- **Conclusion:** click-driven lazy filling is ruled out. Also ruled out, by
  combination with Experiment A: pre-delivery waiting time (60s) is not
  sufficient. What remains: a fill mechanism keyed to very long uptime
  (10+ minutes to hours), or a client-internal asset load that we have not yet
  caused to fire.

### Why these two experiments matter more than their verdicts

Both experiments were *clean*: the server logs, the operator actions, and the
memory snapshots all line up, so the negatives are trustworthy. A negative
result with a broken instrument would teach us nothing; a negative result
with a verified instrument teaches us exactly which hypotheses to abandon.
Moreover, the two experiments together shrink the candidate space — the
catalog is not time-if-60s, not click-driven, and the records are not missing.
The next experiment (long idle hold with periodic rescans) is the natural
successor, and its cost is merely patience.

## Appendix F — The phrases you must never say in a meeting

A short list of statements that have been confirmed false and should not be
repeated as though they were open questions:

- "The client dies waiting for GameGuard." — No; it has already decided.
- "Opcodes 14 through 99 are no-ops." — No; the dispatcher forwards them; only
  four handlers are no-ops.
- "The tree only has two cards in it." — No; the probe's printer lied.
- "The catalog is empty because the cdds are missing." — No; all fourteen are
  in the heap.
- "EM/TD are absent because their cdd is missing from the catalog." — Possibly,
  but then so would 32202 be absent; the cdd is shared.
- "port 2270 speaks ProudNet." — No; it speaks AniPark, same as the auth gate.

## Appendix G — Rules of engagement (for whoever continues the work)

These are the practical instructions that keep this reverse-engineering
project on the right side of its own ethics and its own sanity:

1. **The client is the oracle.** Every reply we send must be traceable to a
   measured behavior or a documented reverse of the client's own code. No
   invented frames.
2. **Verify before trusting probes.** A probe printout is a report, not a
   fact. When a scan result surprises you, fix the filter, rerun, and re-read
   — the 2.8 episode is the standing warning.
3. **Never trust stale addresses.** Session-based addresses rot between
   sessions. Re-derive bases before a scan; a zero read is usually a wrong
   base, not an empty structure.
4. **Keep the twelve-player mapping intact.** The cdd mapping in §2.3 is the
   product of hard-won measurement; preserve it in any refactor and treat it
   as data, not code.
5. **Document negatives.** The disproven theories in Appendix D are worth as
   much as the wins. Every experiment that failed is a stair that no one else
   must climb.
6. **Respect the size law.** 16 records, ~3.5 KB. The client reads it; it
   reads almost nothing larger. Do not re-introduce the 12 KB envelope.
7. **No direct quotes of the human oracle.** When a tester reports a result,
   paraphrase it in the record. The record belongs to the project, not to any
   one conversation.
8. **Censor the document's edges.** No absolute paths, no external project
   names — this document survives contact with the outside world precisely by
   omitting its own directions.