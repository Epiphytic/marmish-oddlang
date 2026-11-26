# Typed native status replies

The [typed status package](../native-status-typed-package.json) builds the explicit-socket client with ordered input and declared command and stream reply types. The envelope accepts a map or three through five positional values. Generated serialization emits an ordered map, with nulls for the ordinary optional result and error fields and omitted nulls for the two typed extensions.

Stable declarations cover every command and stream reply variant in the pinned native source. Nested types include the printable handle fields, bounded Copy Launch paths, inspectable stream state, and receiver claims. Unit cases of internally tagged enums accept and discard extra fields as observed against the original. Named cases, nested records, enumerations, input widths, and reply bounds use the graph declarations. The general `result` field retains its original arbitrary JSON shape.

The package extends the [explicit status client lane](../native-status-v1/README.md) and uses its CLI dispatch and argument guards. [Source pins](provenance.json) · [Reply obligations](reply-obligations.json) · [Native verification](../../../verification/whole/status/typed-client-README.md)
