# Public inherent members of the six core types

The [portable members](../../../oddlang/portable/core-structure-v1/members.ttl)
assign six operations and four constants to stable owner type IDs. The same
file records Rust member spellings under separate binding IDs. Constructor,
accessor and codec roles resolve through the component overlays. The constant
roles select the domain predicates for their values. The two aggregate error
types have no public inherent members in this observation.

The verifier independently builds the original and generated core library.
It compares the full ownership paths from the two Rustdoc observations to
the portable map and checks each projection role against the pinned target
overlay. A role mutation changes encoding to decoding under the same method
name and must fail. A [downstream caller](caller.rs) type-checks each of the
six function pointer assignments and four constant types; changing the
maximum's type or spelling must fail with the expected diagnostic on each
library. These checks exercise member resolution and callable types. They
do not run the methods or compare their returned values.

The release observer includes documentation-hidden items. It measures what
Rustdoc indexes under `cfg(doc)`. Trait members, generic or specialized
receivers, and items excluded from Rustdoc are outside this measurement.

To replay the observation on either qualified ARM64 host, check out the
[Rust observer](https://github.com/Epiphytic/oddlang/commit/b832a8cd9414f15a2c698761029cb76044da9a8f)
and original revision `af54339d250290490d62d4475a77539b7e739071` into
clean worktrees. Generate the standalone candidate from the pinned
[core package](../../../oddlang/portable/core-package.json) with Oddlang
`8a20ea283d0d43da42f054a7c7774eb9e6a20da7`. Pass its emitted directory
as the third argument. Use Rust 1.97.1 with rustc-dev and cached Cargo inputs.

```sh
cargo +1.96.1 run --locked --offline --manifest-path verification/whole/core-lexical/Cargo.toml --bin core-inherent -- \
    "$OBSERVER" "$ORIGINAL" "$CANDIDATE" /tmp/core-inherent.json
```

The verifier checks the original revision and complete generated package
before and after observation. It compares each ordinary compiler/Rustdoc
report with the earlier [lexical audit](../core-lexical/), including its
toolchain, profile, manifest and dependency identities. The receipts include
the complete optional report digest, measured member inventory, independent
caller results and input pins for each host.

The historical Python observer at `ff480c701e02f1d11530303193990d643c1c7da9` remains accepted for replay of its original receipt. The verifier pins the actual observer revision before and after capture.

On macOS ARM64 the pinned Rust observer reproduced the original and generated inherent reports, caller diagnostics and provenance fields. The new receipt records the Rust producer revision. Each Rust observer build uses Cargo 1.96.1 in a temporary directory outside the pinned checkout.

The Rust verifier parses the portable ownership and member graphs, checks the corresponding overlay spellings and applies the swapped-role mutation in memory before compiling its callers. On macOS ARM64 its complete receipt matched the earlier caller's output for both accepted observer revisions. [Rust caller qualification](../core-lexical/rust-qualification-macos-arm64.json)
