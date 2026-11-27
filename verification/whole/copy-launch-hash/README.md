# Verify Launch digest and namespace

Check out the Marmish original at `af54339d250290490d62d4475a77539b7e739071` and Oddlang at the framework revision recorded in [the receipt](reference-aarch64-apple-darwin.json). With Rust 1.97.1 and registry dependencies cached, run from the companion root.

```sh
python3 -B verification/whole/copy-launch-hash/verify.py /path/to/original /path/to/oddlang /tmp/launch-hash-receipt.json
```

The verifier checks the extended model against the recaptured data release, stages only pinned graph, binding and Cargo lock inputs, and builds the native package offline. The macOS build runs with networking and original and companion reads denied. The verifier tests the read denial before building. An unchanged Rust caller then runs against the original and generated libraries. A separate [Rust oracle](cases.rs) defines the expected bytes and errors independently of both package implementations.

The 227 requests cover all mode, direction, codec and flag combinations, independent identity mutations, binary and boundary-length paths, 26 invalid digest inputs and 128 deterministic randomized records. Namespace output is checked on every request, including invalid records. The previous 45 construction and 56 exact decode cases also run against the extended library. Swapping fixed-width slots in the graph changes both outputs to independently predicted values. Native receipts cover macOS and Linux ARM64.

On Linux ARM64, provision Rust 1.97.1, bubblewrap, setpriv and the registries for the pinned locks. The build runs in a separate network namespace with zero effective capabilities and an allowlist of inputs. The original and companion checkouts are inaccessible to the generator and native build. Run the same verifier and compare with the [Linux ARM64 receipt](reference-aarch64-unknown-linux-gnu.json).
