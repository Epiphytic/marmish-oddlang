# Source-derived copy frame encoder

The pinned Rust compiler recovers all eleven frame tags and the 25 ordered writes in the top-level encoder. The release's vacant template retains stable record and field identities, limits and errors whose semantics were independently qualified. The recovered graph reproduces the released frame layout. The separate [obligation report](../../../oddlang/portable/source-copy-frame-v1/obligations.json) identifies the cases that still rely on reviewed guard or nested-fragment behavior.

The verifier changes the upstream `Finish` tag from 9 to 19 and reverses the two 32-byte fields in its encoder. A new compiler observation changes the portable tag and payload ordering. An independent byte oracle checks all eleven variants on both upstream revisions and the two regenerated libraries. Generation and compilation block reads of both upstream source trees and the compiler receipt.

Run from this repository with the upstream revision `af54339d250290490d62d4475a77539b7e739071`, the [recorded framework revision](reference-aarch64-apple-darwin.json) and Rust 1.97.1 with its host `rustc-dev` component.

```sh
python3 verification/whole/source-copy-frame/verify.py \
  /path/to/upstream /path/to/oddlang /tmp/source-copy-frame.json
```

This receipt covers the encoder on macOS ARM64. The mutated reference decoder is outside the comparison. The earlier [copy frame release](../copy-frame/README.md) records the unchanged original decode and error suites.
