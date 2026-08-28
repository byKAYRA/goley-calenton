# goley-shim

`goley-shim` is the small DLL loaded into the fixed Windows client by
`goley-boot`. It observes named kernel-object waits, prevents the client from
silently terminating itself during discovery, and can signal **one explicitly
configured** GameGuard-ready event. It does not forge login/authentication
traffic. Its optional network hook only redirects the one measured legacy
entry-server endpoint to an explicitly configured local listener.

## Target

The release DLL must be built as 32-bit MSVC because the client is 32-bit:

```powershell
cargo build -p goley-shim --release --target i686-pc-windows-msvc
```

Entry redirection is excluded from the default build. Compile it explicitly
for capture runs:

```powershell
cargo build -p goley-shim --release --features netredirect --target i686-pc-windows-msvc
```

Observed local client fixture (the file itself is not part of this repository):

| Role | File | Size | SHA-256 |
| --- | --- | ---: | --- |
| Launcher | `Goley.exe` | 2,691,792 bytes | `A96DB4DC7CB5437AF42AEC5E2ACB2A975377C831C823B17B689F837F31910A82` |
| Packed x86/TLS client | `BinaryTr/BinaryTr.bin` | 8,311,504 bytes | `C136E751905FBE60CF27A71D7D05FBDE7C0428484D577F3A0A56766C839EFCFA` |

The installed fixture does not contain a `Goley_.exe`; runtime research must
target the measured packed client above rather than inventing that filename.

The second known client build has not been supplied, so its hash remains
unknown. Static patches must not be added for it until its exact hash and
original bytes have been measured.

## Boot-to-shim configuration

`goley-boot` passes one JSON object in `GOLEY_SHIM_CONFIG`. See
[`ShimConfig`](src/config.rs). Two named manual-reset events form the loader
handshake:

1. `loaded_event` is signalled as soon as the worker thread has parsed config.
2. `ready_event` is signalled only after unpack readiness, optional validated
   patches, and hook installation have all succeeded.

`gameguard_ready_event` must come from a prior `capture-waits` report. It has no
default and no GameGuard object name is embedded in this crate.

## Entry redirection contract

The redirector is default-off twice: the `netredirect` Cargo feature must be
present and `entry` must be set at runtime. The only accepted replacement form
is `127.0.0.1:PORT`. The hooks cover both `ws2_32!connect` and
`ws2_32!WSAConnect`, but rewrite only the measured
`213.74.179.12:2270` destination. Every other address and port is forwarded
unchanged; adding `20260` or any other route requires separate measurement and
an explicit allowlist change.

Each rewrite emits a synchronous JSONL event with
`event_type=network_connect_redirect`, `api`, `original_destination`,
`redirected_destination`, socket value, and caller module/offset/address. If
`entry` is malformed, non-local, supplied outside run mode, or requested from a
build without the feature, shim startup fails before READY instead of silently
using the public legacy destination.

## Static patch policy

Patch data lives only in [`patches/patches.toml`](patches/patches.toml). The
loader chooses records by full-file SHA-256, validates every original byte,
then applies the selected set. A mismatch rejects the complete set.
