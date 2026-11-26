# Upstream local command owner handoff

Before peer transport, the mainline client contacts its own daemon over a same-user Unix socket. This replay observes the real pinned upstream client against a test-owned socket. It reports the local protocol only. It creates no credentials, opens no peer transport, and executes no fixture command.

For both foreground and detached submission the client requests `status`, then sends `command_prepare` with the resolved account and relay policy. The test owner returns a typed capacity refusal and the client maps it to exit 255, empty stdout, and the exact `marmish: command_room_capacity` diagnostic. An invalid JSON response produces the pinned `ipc_frame_json` diagnostic. For `wait --latest`, the client requests `status` and then `command_invocations` starting with a null cursor and a limit of 64. An independently supplied empty page produces `command_not_found`. The socket fixture compares each frame header and body before sending a reply and confirms request half-close.

Run the capture with the original executable matching the existing [bridge release receipt](../bridge/aarch64-apple-darwin.json). Seatbelt permits only the fixture socket for outbound access and limits filesystem writes to the private test workspace and the null device. The test's relay URL is configuration input and is never contacted.

```sh
python3 -B verification/whole/basic-command-interop/client_owner_preflight.py \
  "$MARMISH_BIN_DIR/marmish" "$OUTPUT"
```

The [macOS ARM64 transcript](upstream-local-owner-aarch64-apple-darwin.json) records the frames and streams as exact bytes in hex. The [generated preflight](generated-preflight-aarch64-apple-darwin.json) independently observes the missing generated CLI handlers. Together these observations identify the first unimplemented client handoffs. They leave generated owner behavior and mixed-peer execution open.
