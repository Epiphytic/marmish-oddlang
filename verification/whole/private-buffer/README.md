# Private byte reader

The prototype crate consumes queued bytes through a private bounded reader. Its Rust declaration has a generic input, an owned 8,192-byte erasing array and two cursor fields. The portable graph records the field order and capacity together with the chosen transitions. Rust names and the type parameter appear in the target overlay. The crate’s bridge and executable have separate contracts.

The recovery input is the pinned Rust compiler's type observation. The field-role assignment and behavior policies come from the reviewed selection. Runtime checks exercise those policies independently. The native build pins the zeroize dependency for storage erasure. The caller directly observes wiping consumed bytes. Erasure of unread bytes on destruction relies on that provider's drop contract.

## Recorded behavior

| Operation and prior state | Observation |
| --- | --- |
| Read two bytes from a three-byte input | The output receives the first two. The reader zeros those two storage slots and retains the third. |
| Consume more than remains | Consumption stops at the end of the available bytes. It performs no upstream read. |
| Read into an empty output | An exhausted reader still polls its input. A previous zero-byte return allows a later poll. |
| Input returns an error after a prior window was exhausted | The reader propagates the error after resetting the beginning cursor. The old end remains. In the scripted input, the next view exposes the zeros from the consumed window before another upstream poll. |

The error case uses a scripted in-memory input that returns Interrupted. A slice-backed cursor does not produce this error. A consumer retrying the read can receive zeros in place of fresh input. This is a compatibility obligation of the pinned release.

The error observation matters for reproducing the next call as well as the current one. The upstream source controls independently remove the consumed-byte wipe and clear the end before a failed refill. Each must fail the corresponding assertion. The baseline runs the original private-reader unit test and three project-authored observations. [macOS baseline](original-aarch64-apple-darwin.json) · [Linux baseline](original-aarch64-unknown-linux-gnu.json)

To repeat the original observations, use a clean reference checkout at `af54339d250290490d62d4475a77539b7e739071` and an offline Rust 1.97.1 dependency cache.

```sh
python3 -B verification/whole/private-buffer/baseline.py "$UPSTREAM" "$CARGO_TARGET" /tmp/buffer-original.json
```
