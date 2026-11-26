# Client restart with a retained poll

Both pinned clients detach a fixed invocation against a test-owned Unix
socket. A separate wait process requests a poll at committed cursors
`(8, 9)`. The fixture compares the exact requests, retains the poll's
terminal answer and kills that process before it replies. A fresh wait
process then opens the same private profile with the exact detached handle.

The mainline wait reads the retained entry and answer, looks up the committed
cursors, writes only the remaining bytes and consumes the answer using
`(8, 9)` as the expected pair. It exits 7 and the fixture advances both
cursors to 16. The sealed generated wait stops after listing the retained
entry. It exits 255 with the exact handle line and
`marmish: command_unconsumed`, emits no stdout and leaves both cursors at
their earlier positions.

The [receipt](pending-poll-restart-aarch64-apple-darwin.json) attests to the
client restart and IPC behavior. The in-memory fixture supplies the retained
answer, committed cursor state and private resume manifest. Owner process restart, journal recovery
and encrypted peer delivery require separate evidence.

```sh
python3 -B verification/whole/basic-command-interop/pending_poll_reference.py \
  "$UPSTREAM_CLIENT" "$SEALED_CLIENT" \
  verification/whole/basic-command-interop/bounded-local-progress-build-aarch64-apple-darwin.json \
  "$RECEIPT"
```

Claude Opus 5.5 auto review approved the frame checks, isolation and scope.
