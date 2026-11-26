# Poll response chunks

The portable response schema describes a stream chunk shared by stdout and stderr. The independent consumers decode and re-encode the same JSON with the generated Rust type and the pinned mainline Rust type. Cases include arbitrary binary data, cursor values above the JavaScript safe-integer range, the largest u64, the 16,384-byte payload bound, malformed base64, missing fields and duplicate keys. The comparator checks each consumer against explicit expected bytes or rejection.

Run from the companion checkout with a native Oddlang compiler and a checkout containing the pinned mainline revision.

```sh
python3 verification/whole/command-responses/verify.py "$ODDLANG" "$MAINLINE_CHECKOUT"
```

The original source is used only in the reference consumer. This isolated type comparison covers chunk structure. Whole responses additionally bind the request digest, operation, status, and per-stream cursor constraints. Live qualification also requires an attributed receiver result. [Chunk observations](aarch64-apple-darwin.json)
