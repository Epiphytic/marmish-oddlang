# Verify launch data and validation

Check out the original at `af54339d250290490d62d4475a77539b7e739071` and the framework at `5f4cf958b7b4f8da8f8309f87734c14a8309c7d7`. From the companion root, with Rust 1.97.1 and registry dependencies cached, run

```sh
python3 -B verification/whole/copy-launch/verify.py /path/to/original /path/to/oddlang /tmp/launch-receipt.json
```

The verifier builds its own generator, stages only pinned data and the Cargo lock, generates and compiles the native library, and checks the resulting package lock. On macOS the generator and library build run under a sandbox which denies networking and reads from both original and companion repositories. A control attempts to read each denied repository. Other hosts report the filesystem isolation actually exercised.

The source-recovery stage runs separately and compares the frozen inputs with a fresh extraction. An outside Rust caller compiles against both libraries, destructures every field and compares 45 native constructions with an independent JSON/validation oracle, 56 decode requests with the original, and all 1,280 possible byte conversions across five enums. Decode observations include exact Serde diagnostics and successful re-encoding. The final negative control changes guard order and verifies the changed observable error.

This exercises the launch API described in the [contract](../../../oddlang/portable/copy-launch-v1/README.md). Digest operations, manifests, checkpoints and the complete stream and binaries require further qualification.
