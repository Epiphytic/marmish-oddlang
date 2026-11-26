# Pinned client local delivery reference

The pinned upstream client runs through a test-owned Unix socket in a private Seatbelt workspace. The fixture supplies the known command owner replies and performs no command execution. It compares every request byte on the socket, independently reconstructs canonical submit and poll digests, and validates the handle, session, request ID and cursor bindings.

The reference submits through readiness and room preparation. A queued reply advances to the unconsumed response lookup. The client consumes the original submit acknowledgment before polling. The terminal poll carries distinct binary stdout and stderr, including NUL and 0xff. The client consumes the poll at the exact earlier cursor pair and exits 7 with the supplied bytes. The detached operation returns the exact handle once the submit is accepted. A separate wait in the same private profile resumes that handle and receives the same bytes and exit. A corrupt submit response digest fails before any consume request.

```sh
python3 -B verification/whole/basic-command-interop/client_delivery_reference.py \
  "$MARMISH_BIN_DIR/marmish" "$OUTPUT"
```

The [macOS ARM64 receipt](upstream-local-delivery-aarch64-apple-darwin.json) records exact binary outputs, status, and the ordered local calls. Temporary handles and per-run identifiers are validated in memory. The receipt omits their values. This fixture characterizes the local caller contract. It has no authenticated peer or worker.

Claude Opus 5.5 approved the staged reference in Claude Code auto mode after checking exact frames, canonical digests, cursor and handle bindings, corrupt digest refusal, and confinement. The static review and the runtime replay ran independently.
