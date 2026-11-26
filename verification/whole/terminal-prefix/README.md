# Leading global options before terminal help and usage

The seven-member successor changes the two CLI grammar graph pins in the
converged native package. Its other six member records, root Cargo lock,
sibling edges, native component pins and CLI bindings are inherited.

The recovery checks a selected source predicate, its direct denial branch
and the raw error formatter. It recovers the allowed global identifiers and
exact diagnostic from a pinned original Rust module. The generic projection
uses primary grammar metadata for option names and arity. It recognizes
long and short prefixes with attached or separate values. Denied prefixes
terminate with the recovered diagnostic. For allowed prefixes the auxiliary
parser must return help or usage before the projection supplies a response.
The CLI selector remains unbound.

Prefix clusters, malformed prefixes, separators and successful auxiliary
parsing remain outside the projection. The probes supply no remote host or
command. The separate `--debug rsh --version` control is syntactically
accepted by the auxiliary parser as a positional value. The original then
rejects the leading dash during host validation. The candidate retains its
primary parser error. This post-parse difference remains uncovered. Both native
hosts compare exact stdout, stderr and status against their own verified
original executable hashes in a network-denied, read-only process sandbox.

## Reproduce

Use clean input, framework and original checkouts at the revisions recorded
in the receipt. The verifier builds its compiler with Rust 1.97.1 and
builds the staged workspace graph without reading those source checkouts.

```sh
lane=verification/whole/terminal-prefix
python3 -B "$lane/verify.py" "$INPUT_REVISION" "$FRAMEWORK" "$COMPILER_TARGET" \
  "$ORIGINAL" "$ORIGINAL_BIN" "$RECEIPT"
```

The Darwin verifier optionally takes an external `--candidate-target` to
reuse build dependencies. Linux builds use a temporary isolated target.
The receipts identify the executable hashes actually exercised on each
host. Compare the recorded cases and sealed workspace lock across replays.
