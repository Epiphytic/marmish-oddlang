# Native command response shapes

The portable response schema describes stream chunks, status, admission, dispositions, outcomes, gaps and typed errors. Stable enum case IDs carry the wire spelling, while the Rust binding supplies the native name. The independent consumers decode and re-encode the same JSON with the generated Rust type and the pinned mainline Rust type. Cases include arbitrary binary data, cursor values above the JavaScript safe-integer range, the largest u64, the 16,384-byte payload bound, malformed base64, missing fields and duplicate keys. Additional cases verify required nullable members, exit code bounds, renamed enum cases, nested records and field ordering. The comparator checks each consumer against explicit expected bytes or rejection.

Run from the companion checkout with a native Oddlang compiler and a checkout containing the pinned mainline revision.

```sh
python3 verification/whole/command-responses/verify.py "$ODDLANG" "$MAINLINE_CHECKOUT"
```

The original source is used only in the reference consumer. This isolated type comparison covers the structural types and their Serde representation. Whole responses additionally bind the request digest, operation, status, and per-stream cursor constraints. Live qualification also requires an attributed receiver result. [Structural observations](aarch64-apple-darwin.json)

The command response's `Gap.end` must appear on the wire and can contain null. This field has its own stable ID. The stream component's `MissingRange.end` accepts omission as well as null. The structural cursor and gap fields each represent u64; the command envelope applies further range and ordering rules before accepting a complete poll response.
