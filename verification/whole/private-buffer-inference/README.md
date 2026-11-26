# Inferring the private reader's transitions

The structural selection names the compiler declaration, the portable identity and the four field roles. It contains no policy answers. A native test supplies three fresh input streams, records observations, and passes them to Oddlang's buffered-input inference command alongside the compiler's type observation.

| Input stimulus | What distinguishes the behaviors |
| --- | --- |
| Read into an empty destination, then peek | The input counts whether the empty read polled it. |
| Consume part of a nonzero payload, then its remainder | The input sees either zeros or the previous payload in the next supplied buffer. The observer also verifies the unconsumed suffix. |
| Fail the first refill after consuming a payload | A retry exposes a stale window after two input polls or returns EOF on the third poll. The byte values must agree with the consumption observation. |

The verifier changes each policy independently in a disposable copy of the original and then changes all three together. It captures the five native traces again, compares them to the released observations and confirms that the combined edits preserve the selected compiler declaration. Each inferred graph must rebuild a native reader that emits the same trace. The release-erasure obligation still relies on the native storage provider.

The native test observes the behavior of a known private type. Its adapter names, scripted stimuli and semantic field roles are supplied by the project. Inference distinguishes the supported alternatives for that profile. Inputs outside those observations still need their own compatibility evidence.

```sh
python3 -B verification/whole/private-buffer-inference/qualify.py \
  "$UPSTREAM" "$ODDLANG" "$COMPILER_CACHE" "$ORIGINAL_CARGO_TARGET" /tmp/observed-reader.json
```

The native qualification requires clean checkouts of the original and the compiler named in [compiler.json](compiler.json). Rust 1.96.1 builds Oddlang, and Rust 1.97.1 runs the native cases. On each ARM64 host, all five inferred graphs produced the observed bytes, errors and poll counts for all three trials. Each graph and native package lock matched between hosts. Both runs used companion verifier commit `3703f0340f3535b0625928da0b89e255dff0c70d`. [macOS results](candidate-aarch64-apple-darwin.json) · [Linux results](candidate-aarch64-unknown-linux-gnu.json)
