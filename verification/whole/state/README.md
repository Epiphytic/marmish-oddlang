# Bridge state compatibility fixture

The pinned original wrote a fresh v1 state, shut down and reopened a copy. The public fixtures pin exact stdout bytes, stderr and status for initialization, two maintenance limits and cleanup readiness. Reopening retains the same engine key. Replacing the copied database with invalid bytes must yield the named storage error followed by three uninitialized-request errors.

The original database and generated key remain together in a private capsule outside Git. The checked receipt pins its manifest. A downstream reader only needs its own two executables, the public test and the independently retained capsule. Verification copies it before each run.

```sh
cargo +1.96.1 run --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml \
    --bin bridge-state-replay -- read /path/to/bridge /path/to/worker /private/capsule
```

To qualify a new original fixture, run `capture` with an absent output directory and a verified original pair. Supply its matching build receipt with `--writer-receipt`. Save the resulting JSON and use `--state-receipt` on later reads. The initial fixture covers a cleanly closed v1 store without jobs. Populated job history, SQLCipher and interrupted writes require separate fixtures.
