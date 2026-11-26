# Structural extraction baseline

Run the companion replay from a clean Oddlang checkout at `b653f1ada68791cd2fb70293bf227c5665850d1e`, a clean selected Marmish checkout and the Rust 1.97.1 compiler development component.

```sh
python3 replay.py /path/to/oddlang /path/to/marmish /tmp/fresh-projections \
    --triple aarch64-apple-darwin
```

Each output directory contains a graph of candidate structural facts and a lock binding the compiler, input files, Cargo profile selection and graph digest. The checked-in macOS locks came from the release-mode adapter at the new source pin. The stream graph retained its earlier semantic digest. The bridge binary and its processing library have separate identities, graphs and limitations. Gap rows describe unresolved structural observations per owner. These three locks cover part of the workspace. The behavioral conformance evidence lives in sibling directories.
