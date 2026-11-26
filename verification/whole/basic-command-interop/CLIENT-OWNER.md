# Generated native client owner handoff

The [client candidate workspace](../../../oddlang/portable/workspace-basic-client-owner.json) adds the [local workflow](../../../oddlang/portable/basic-client-owner-workflow-v1/graph.json) to the five pinned wire request codecs. The normal `marmish` remote entry resolves authorized targets from a private existing profile. It checks the configured canonical relay set and routing policy and accepts bounded argv and binary stdin. It then uses the explicit or profile-discovered same-user local socket.

The generated client sends readiness, room preparation and submit through its local IPC path. A ready reply binds the actual session into submit. The tested typed refusal paths preserve exact status and streams. The shown scope requires an explicit state directory, an npub destination, the argument separator and `--no-autostart`. The phase does not execute a command. A queued reply still reports unavailable completion. The workflow needs handle binding, poll, consume and a generated command owner for full interoperation.

The [generated client receipt](generated-client-owner-aarch64-apple-darwin.json) compares five fixed benign local transcript cases against the pinned upstream witness and one default endpoint case against its expected frames. The local owner is test-owned and the clients cannot connect to any other socket. Candidate-only authority, document and payload failures stop before the local socket. The test measures local handoff and exact refusal behavior. Setup, remote peer traffic and command execution are separate qualification gates.

```sh
oddlang build native-workspace rust oddlang/portable/workspace-basic-client-owner.json "$OUTPUT"
cargo +1.97.1 build --manifest-path "$OUTPUT/Cargo.toml" --workspace --release --locked --offline
python3 -B verification/whole/basic-command-interop/generated_client_owner.py \
  "$OUTPUT/target/release/marmish" "$EXPECTED_CLIENT_SHA256" "$RECEIPT"
```
