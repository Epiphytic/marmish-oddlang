# Local relay fixture

The pinned upstream workflow uses Deaddrop commit `b38b2841b081952dc7728da391fe0e26fc7c6181`. For deterministic transport checks we use its explicit loopback debug mode. Its three reference tests cover an authenticated publish that finishes after sender disconnect, challenge ordering in a pipelined exchange, and process-level startup, logging and shutdown. In these tests ephemeral ports, challenge values and wall-clock timestamps may vary. The required response ordering, acceptance and redaction checks stay fixed. These checks qualify the local relay fixture. Actual original/generated Marmish peers still require separate runs.

From a clean checkout of that Deaddrop commit, populate Cargo's pinned dependency cache with `cargo +1.97.1 fetch --locked`. Then run the companion verification script.

```sh
python3 verification/whole/relay/replay.py /path/to/deaddrop /tmp/deaddrop-build
```
