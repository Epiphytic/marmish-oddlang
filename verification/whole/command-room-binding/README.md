# Bind a decoded command frame to an admitted room

The command wire binding contains the session, client account and server account carried by a packet. Each role checks those three values against its own admitted session and its local and remote accounts. A server reads a request with the remote client as sender. A client reads a response with the remote server as sender. On mismatch either method returns `Invalid("command_wire_binding")`. Supply the three expected values from the authenticated owner. Build the claim from the decoded wire fields.

The portable record and both role-specific comparisons are in `oddlang/portable/command-room-binding-v1`. The native binding uses `command_binding::WireBinding`. Oddlang projects these using its existing wire-schema and field-relation components.

The archived macOS ARM64 run used the compiler built from Oddlang commit `a33d0d0aa77d51ba57c35272da41e9ba6da3e61f`. The receipt identifies the executable by SHA-256. Run the compiled witness from the companion checkout.

```sh
cargo +1.97.1 run --locked --offline --quiet --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin check-command-room-binding -- "$ODDLANG"
```

The Rust verifier reads the exact packet bytes frozen from the historical request corpus (the canonical case per operation) and response corpus (the success case per operation). Ten canonical packets cover all five request operations at the server and all five response operations at the client. The harness keeps the expected room facts fixed while it changes the packet's session, client, server, or both accounts together. The regular wire decoder must accept every structurally valid packet. The subsequent room check must admit ten and reject forty with the specified diagnostic. [Generated observations](aarch64-apple-darwin.json)

The pinned native implementation performs this comparison after checking the native message source, author, group state, event kind and tags. The mainline test `encrypted_command_bytes_require_the_native_session_binding` exercises an encrypted submit packet and a wrong-session packet. The generated witness exercises the field comparison. Native event provenance and live receiver attribution are separate requirements. [Native comparison](https://github.com/Epiphytic/marmish/blob/af54339d250290490d62d4475a77539b7e739071/crates/marmish-native/src/command_adapter.rs#L414-L432) · [Native encrypted-session test](https://github.com/Epiphytic/marmish/blob/af54339d250290490d62d4475a77539b7e739071/crates/marmish-native/src/mdk_adapter_tests.rs#L1325-L1384)
