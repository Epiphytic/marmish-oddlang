# Original release binaries and bridge baseline

This runner checks the original Marmish release at `af54339d250290490d62d4475a77539b7e739071`. The earlier five-case bridge fixture remains an independent expectation. Its recorded upstream hash identifies the release on which those expectations were established. This runner verifies the original source tree, Cargo.lock, oracle driver and fixture hashes, builds eleven executables, then checks the new release against that fixture twice.

Use Python 3.11 or later and Rust 1.97.1 on a native host. Supply a clean checkout of the pinned upstream, an Oddlang checkout with the driver SHA-256 enforced by `baseline.py`, locally available locked Cargo dependencies, and a writable Cargo target directory. Run as a regular user with a hard descriptor limit supporting the 1,048,576 case.

```sh
python3 -B verification/whole/bridge/baseline.py \
  --upstream /path/to/pinned/marmish --oddlang /path/to/oddlang \
  --target-dir /path/to/original-build --receipt /tmp/original-bridge.json
```

The default release build includes the client, daemon, native worker, release tool, engine worker, trusted bridge and keychain helper. Separate Cargo selections produce the MDK probe, fault frontend and two installed tools. The receipt records the selection and binary hashes for each build. Hashes identify the observed executables and can vary with build paths. Test inputs select synthetic principals and temporary state. Only the realized temporary and worker path fields are normalized before repeat comparison.

On macOS ARM64 the bridge returns `worker_unavailable` for the otherwise successful Init when the child inherits a soft and hard open-file limit of 1,048,576. The check covers the exact reply and channels, exit status and remaining state tree. At 256 descriptors the fixture checks a live lock, three database files, key layout, named framing/validation errors, retry and duplicate request effects, exit status and lock release. Linux ARM64 successfully initializes at the high limit and leaves the expected closed state. The macOS receipt was replayed with the Linux-aware lock probe. The driver checksum is the only change from its receipt at companion commit `e58312b`.

The separate macOS effect observation applies a kernel sandbox policy allowing filesystem writes only inside the disposable state path. A denied sibling-file creation validates that policy. The test watches kernel process events from immediately before the Init request until bridge exit and samples live descriptors, direct children and the filesystem lock after the reply. The checkpoint lists open files and the process event set records whether the bridge forked, executed another image or exited during that interval. Transient file opens, rejected write attempts by the bridge, and changes to child resource limits after exec require additional tracing. Each receipt qualifies only the named host and observed interactions.
