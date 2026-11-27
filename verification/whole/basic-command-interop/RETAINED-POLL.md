# Retained local poll recovery

The [retained-poll workspace](../../../oddlang/portable/workspace-retained-poll.json)
binds a resumed wait to the fixture owner's retained poll entry. The client
checks the entry digest, typed response, handle, session, peer accounts and
invocation. It then reads the committed cursor pair and recomputes the
canonical poll request hash. Only after these checks does it validate and
release both stream chunks, then request consumption with the previous pair.

The [restart witness](PENDING-POLL.md) supplies the independent upstream
transcript and the previous generated candidate's refusal. The recovery
fixture uses that same interrupted wait boundary. It also supplies distinct candidate-only
refusal controls for an outer digest, a digest-valid wrong session, an inner
request hash and a changed committed cursor. Each stops before output and
poll consumption. These diagnostics and decisions are candidate-specific.
The matching upstream claim applies to recovery of a valid terminal entry. The in-memory owner actor supplies the retained record.
Owner durability and encrypted delivery require separate evidence.

The fixture exercises a single retained terminal entry. Multiple entries and
retry or nonterminal retained replies have their own coverage obligations.

The [cold build](retained-poll-build-aarch64-apple-darwin.json) compiled both
executables with Seatbelt filesystem and network restrictions. The
[recovery receipt](retained-poll-recovery-aarch64-apple-darwin.json) matches
the original client's exact restart calls, stream suffixes, exit 7 and
committed cursor advance. The
[regression replay](retained-poll-progress-aarch64-apple-darwin.json)
matched all eight earlier positive cases and six existing refusals.

```sh
python3 -B verification/whole/basic-command-interop/pending_poll_recovery.py \
  "$SEALED_CLIENT" \
  verification/whole/basic-command-interop/wait-profile-build-macos-arm64.json \
  "$RECEIPT"
```

Claude Opus 5.5 auto follow-up approved the executable and fixture pins,
comparison with the upstream restart, and the bounded refusal claims.

The workspace now selects the local wait configuration diagnostics in companion
revision `8dbb0112dd837622f5ee9b22c32055e10d741854`. Use the matching sealed
client for the replay above. At workspace lock
`1d54bfc91701cb458c920bd54b3bb5297d53d615a8fae3dc77230e6084d14e73`,
the new [build](wait-profile-build-macos-arm64.json),
[interrupted wait](wait-profile-recovery-macos-arm64.json) and
[progress replay](wait-profile-progress-macos-arm64.json) reproduce the earlier
local delivery and refusal results. The file and directory diagnostics apply
before any owner contact. To reproduce the historical receipts, use the
workspace and build pins recorded in those receipts.

The [mainline control](wait-profile-control-macos-arm64.json) completed the first
foreground case with exact output and receiver attribution. On the new lock the
[generated client](wait-profile-mixed-client-macos-arm64.json) exited 255 after
2.08 seconds with the stderr digest of `marmish: command_local_response\n`.
The [mainline client against the generated server](wait-profile-mixed-server-macos-arm64.json)
exited 255 with `remote_pending` after 303.05 seconds. Neither intended receiver
produced a fixture receipt. All three workspaces were removed. Host metadata
checks passed in the control and generated-client runs. A Keychain directory
metadata change during the generated-server run blocked its cleanup audit.
