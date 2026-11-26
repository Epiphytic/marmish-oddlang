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
  verification/whole/basic-command-interop/retained-poll-build-aarch64-apple-darwin.json \
  "$RECEIPT"
```

Claude Opus 5.5 auto follow-up approved the executable and fixture pins,
comparison with the upstream restart, and the bounded refusal claims.
