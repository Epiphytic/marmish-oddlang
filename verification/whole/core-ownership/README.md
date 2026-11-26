# Portable core ownership

The [Turtle map](../../../oddlang/portable/core-structure-v1/ownership.ttl)
assigns six data declarations to the portable configuration and
probe-framing modules. Their type and module IRIs can survive a change
to the Rust module tree. A separate Rust binding records each root
re-export, the two observed declaration paths and the selected portable
component and Rust overlay. The type IRIs identify this ownership map's
declarations. For example, `configuration-failure` and `framing-failure`
identify aggregate types in the map. The existing component graphs
declare individual error cases.

The package, module and binding nodes explicitly map to the
[original Rust facade graph](../../../oddlang/facade/domain.ttl) and its
[import lock](../../../oddlang/facade/import.lock). The facade importer
established the original owners. The [separate native comparison](../core-lexical/)
established the generated lexical declarations and associated sealed
component files on both ARM64 hosts. The older Darwin workspace audit
compiled all six import spellings. The portable owner is a reviewed
correspondence to this pinned evidence.

Use [ownership.sparql.txt](ownership.sparql.txt) with the Turtle map, or run its verifier
from a checkout containing the linked Git revisions:

```sh
uvx --offline --from rdflib==7.6.0 python -B verification/whole/core-ownership/verify.py
```

The verifier parses the native facade and portable ownership graphs,
reconciles the two-host reports, verifies the pinned manifest and locks,
and runs the query. Its negative control moves `retention-policy` to
the existing probe-framing module while keeping the same native binding
and rejects the wrong owner. The [query result](observed.txt) records the
type ID, portable owner and import spelling for each row, followed by the
negative-control outcome.
