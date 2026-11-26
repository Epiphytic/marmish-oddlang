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
