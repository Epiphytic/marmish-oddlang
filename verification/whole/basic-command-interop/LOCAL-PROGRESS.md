# Bounded local command delivery

The [delivery workspace](../../../oddlang/portable/workspace-basic-command-delivery.json)
combines the prepared foreground owner and generated typed command response
codecs with the normal Marmish submit and inline wait handlers. The submit
handler validates the prepared session and queued handle against its private
profile context. It checks the original submit reply before requesting its
consumption. Each poll uses a new request identity and the acknowledged
cursor pair. The handler checks peer, session, request identity, request hash,
invocation and authority inside the typed response, independently of the
outer response digest. It checks both stream byte windows before output,
then acknowledges delivery with the earlier cursors. Inline wait resolves a
strict handle against the selected private profile and applies the same
checks to subsequent responses.

The [upstream local oracle](upstream-local-progress-aarch64-apple-darwin.json)
uses the pinned release client and a test-owned Unix socket. It checks exact
requests, two-poll foreground, detached wait from both zero and committed nonzero
cursors, sixteen polls carrying the
fixed 128 KiB streams, the separate Finish exchange, the exact detached
handle and command denial, and corrupted digest refusal before consumption. The actor supplies the fixture
streams without command execution. The receipt includes stream hashes and
local operations. Handle values and fresh identifiers are validated in memory.

```sh
python3 -B verification/whole/basic-command-interop/client_progress_reference.py \
  "$CLIENT" "$CLIENT_SHA256" "$RECEIPT"
```

Generated server-side command admission, delivery over authenticated peers,
recovery with an already pending poll at process restart, and full upstream
refusal coverage are open qualification gates.

## Composed native build and comparison

The [build receipt](bounded-local-progress-build-aarch64-apple-darwin.json)
pins the native Rust compiler, companion source, workspace lock and both
executables. Seatbelt isolation denied reads of the original source and binary,
the independent oracle fixture and four other probes. The build ran with
network access denied. The receipt records its filesystem access scope.

The [generated client comparison](bounded-local-progress-candidate-aarch64-apple-darwin.json)
matched all eight pinned local cases, including the suffix of both streams
from the prior nonzero cursor pair. Four rejected handle or manifest cases
stopped before owner contact. Nonzero initial cursors for a fresh submit and a
wrong-session response with a valid outer digest stopped before output or
poll consumption. Use `--check-resume-admission` with the generated client to
replay these six additional cases.

The [package codec comparison](bounded-local-progress-workspace-codec-aarch64-apple-darwin.json)
used the emitted `marmish-native` response module pinned by the same workspace
lock. It checks 117 mainline packet observations, including 36 admitted
responses, and rejects all 20 cross-operation decoder pairings.

The bounded progress checker accepts `--scratch-parent DIRECTORY` for the private Unix socket, stdout and stderr files. Select a short absolute path on a volume with room for the two 131,072-byte streams. The default is `/private/tmp`. Both clients must pass against the same fixture before comparing the `cases` objects. This exercise uses a local owner fixture.

At the profile command-policy lock `889017343f0fdb092dee203264aa4f6d6d194bad8ef396e336f2dd2cf6ec9b76`, the mainline and generated clients agreed on all eight cases. The foreground graph in this workspace includes room preparation and policy parsing. The separate authorized-owner-submit workspace declares the local submit operation and queue. To reproduce the diagnostic for this specific lock, append `profile-policy-route-probe.rs` to `marmish-native/src/group_4/__projection_4.rs` in a disposable copy of the verified workspace and run `cargo +1.97.1 test --locked --offline -p marmish-native --lib release_route_probe` there. The probe sends no command and opens no network connection. It observes a structured refusal for the prepare control, no route for the submit operation and an empty queue. [Client and packet observations](profile-policy-local-macos-arm64.json)
