# ProudNet connection-hint A/B probe plan

## Evidence status

This document separates three kinds of information:

1. **Workspace fact:** the TCP framing and core opcode values in `AGENTS.md`.
2. **MIT reference profile:** field order and defaults read from
   `aizuon/nexum`. These values are probe inputs, not observations of Goley.
3. **Target unknown:** whether this client contains NetConfigDto field 8, its
   NetVersion, its protocol GUID, and its RSA-OAEP variant.

No client was launched while preparing this document. A successful live
opcode-5 reply is required before either field layout may be promoted from a
probe hypothesis to a measured Goley fact.

## Opcode-4 payload layout

All integer and floating-point fields are little-endian. Booleans and both
enum-valued fields occupy one byte. Offsets include the core opcode at payload
offset zero.

| Field | Type | Field-8 present offset | Field-8 absent offset |
| --- | --- | ---: | ---: |
| Core opcode (`4`) | `u8` | 0 | 0 |
| EnableServerLog | bool/u8 | 1 | 1 |
| FallbackMethod | raw `u8` | 2 | 2 |
| MessageMaxLength | `u32` | 3 | 3 |
| IdleTimeout | `f64` | 7 | 7 |
| DirectP2PStartCondition | raw `u8` | 15 | 15 |
| OverSendSuspectingThresholdInBytes | `u32` | 16 | 16 |
| EnableNagleAlgorithm | bool/u8 | 20 | 20 |
| EncryptedMessageKeyLength | `u32` | 21 | 21 |
| FastEncryptedMessageKeyLength | `u32` | 25 | absent |
| AllowServerAsP2PGroupMember | bool/u8 | 29 | 25 |
| EnableP2PEncryptedMessaging | bool/u8 | 30 | 26 |
| UpnpDetectNatDevice | bool/u8 | 31 | 27 |
| UpnpTcpAddrPortMapping | bool/u8 | 32 | 28 |
| EnableLookaheadP2PSend | bool/u8 | 33 | 29 |
| EnablePingTest | bool/u8 | 34 | 30 |
| EmergencyLogLineCount | `u32` | 35 | 31 |
| RsaPublicKey length | ProudNet scalar | 39 | 35 |
| RsaPublicKey data | DER bytes | after scalar | after scalar |

The Nexum server creates the public-key field as PKCS#1 DER
`RSAPublicKey ::= SEQUENCE { modulus INTEGER, publicExponent INTEGER }`. It is
not an X.509 SubjectPublicKeyInfo wrapper. A freshly generated 2048-bit key
with exponent 65537 encodes to 270 bytes, so its canonical ProudNet length
scalar is `02 0E 01`.

With a 270-byte key and no trailing bytes:

- field 8 present: payload length 312 (`0x0138`), complete TCP frame begins
  `13 57 02 38 01 04` and is 317 bytes;
- field 8 absent: payload length 308 (`0x0134`), complete TCP frame begins
  `13 57 02 34 01 04` and is 313 bytes.

## Nexum reference probe profile

The following values come from Nexum defaults. They remain deliberately
labelled as reference inputs rather than target facts.

| Field | Reference input | Wire bytes |
| --- | ---: | --- |
| EnableServerLog | false | `00` |
| FallbackMethod | None (`0`) | `00` |
| MessageMaxLength | 1,048,576 | `00 00 10 00` |
| IdleTimeout | 900.0 | `00 00 00 00 00 20 8C 40` |
| DirectP2PStartCondition | Always (`1`) | `01` |
| OverSend threshold | 15,360 | `00 3C 00 00` |
| EnableNagleAlgorithm | true | `01` |
| EncryptedMessageKeyLength | 256 bits | `00 01 00 00` |
| FastEncryptedMessageKeyLength | 512 bits | `00 02 00 00` |
| AllowServerAsP2PGroupMember | false | `00` |
| EnableP2PEncryptedMessaging | false | `00` |
| UpnpDetectNatDevice | true | `01` |
| UpnpTcpAddrPortMapping | true | `01` |
| EnableLookaheadP2PSend | false | `00` |
| EnablePingTest | false | `00` |
| EmergencyLogLineCount | 0 | `00 00 00 00` |

## Controlled A/B procedure

1. Generate one ephemeral RSA-2048 keypair and retain its private half outside
   the repository. Use the same pair in both trials.
2. Start each trial with a fresh client process and fresh TCP connection.
3. Hold every opcode-4 byte constant except the four bytes that represent the
   presence of field 8. Record the exact frame SHA-256 and length.
4. Bound the wait interval and save all received TCP bytes outside the
   repository. Parse framing before interpreting a payload.
5. Classify a layout as accepted only after receiving a complete core opcode-5
   frame whose two scalar-length blobs are internally complete.
6. Treat FIN, reset, or silence as `rejected_or_inconclusive`, not as proof of
   the alternative layout. Both layouts failing leaves the DER form or another
   configuration value as a separate variable to test.
7. After opcode 5, try OAEP-SHA1 and OAEP-SHA256 locally with the same private
   key. Record the algorithm only if decryption succeeds and produces a valid
   AES key length. Do not select the Nexum SHA-256 behavior without this check.
8. Send the opcode-only payload `06`, capture opcode 7, and retain the GUID's
   exact 16 wire bytes plus the `u32` NetVersion. This closes those unknowns
   without using a reference project's constants.

Nexum's opcode-5 reference behavior uses RSA-OAEP-SHA256 for the secure AES
key and wraps the fast key with that AES key. Both algorithm choices still
require live Goley confirmation.

## Clean-room implementation

`crates/proudnet/src/handshake.rs` models the field-8 choice explicitly as
`Absent` or `Present(u32)`. It has no Goley defaults. It also retains trailing
bytes in decoded opcode-4, opcode-5, and opcode-7 messages and applies
caller-selected byte-array limits.
