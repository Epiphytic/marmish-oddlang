# Retained local poll recovery

The [retained-poll workspace](../../../oddlang/portable/workspace-retained-poll.json)
binds a resumed wait to the fixture owner's retained poll entry. The client
checks the entry digest, typed response, handle, session, peer accounts and
invocation. It then reads the committed cursor pair and recomputes the
canonical poll request hash. Only after these checks does it validate and
release both stream chunks, then request consumption with the previous pair.

The [restart witness](PENDING-POLL.md) supplies the independent upstream
transcript and the previous generated candidate's refusal. The recovery
fixture uses that same interrupted wait boundary. It also supplies distinct
invalid cases for an outer digest, a digest-valid wrong session, an inner
request hash and a changed committed cursor. Each stops before output and
poll consumption. The in-memory owner actor supplies the retained record.
Owner durability and encrypted delivery require separate evidence.
