# Client and daemon CLI parser baseline

The transcript freezes 34 invocations of each executable under an empty disposable home and the C locale. The driver discovers the advertised commands from root help, probes help for each, adds the four hidden evidence/cleanup commands, and exercises version and an unknown global option. Each row pins arguments, exit status and both output channels. Changing a help exit status in the fixture is detected. This is a parser baseline. The remote-destination syntax, internal copy helper and stateful operation behavior have separate obligations.

Run `python3 replay.py verify /path/to/installed/bin` with independently staged release binaries. Capturing a new fixture requires the original binaries matching the selected build receipt. Ports, credentials and live services are unnecessary for this check.


## Generated parser-only executables

The [native CLI candidate](../../../oddlang/portable/native-cli-v1/) supplies the graph, executable overlays and workspace pins. Generate and replay both binaries on macOS with:

```sh
verification/whole/cli/macos.sh /path/to/oddlang-checkout /path/to/pinned/original /path/to/new/build
```

The runner builds the compiler offline from a clean checkout of Oddlang commit `c01279eb61ed9e86b51b2eeaf0c5b744397c74a3` using Rust 1.97.1. Recovery reads the pinned original parser from Git. Generation and the locked offline release build run in a fresh staging directory with a Seatbelt file-data allowlist and network denial. Only the graph inputs, runner, exact compiler, Cargo registry, toolchain and system build paths are readable. The allowlist excludes the original checkout, companion checkout, both Git object stores and the frozen transcript. Replay gets the transcript only after generation and build. The standalone `project.py` accepts separate `recover` and `generate` runs, followed by the existing `replay.py verify` command.

The replay qualifies the parser slice for both executable roles. Stateful commands and daemon IPC are uncovered. The [macOS ARM64 receipt](generated-aarch64-apple-darwin.json) records the exact Oddlang revision, toolchain, locks and observation counts. Rust and Clap embed build paths in executable bytes, so hashes vary with the staging directory. Compatibility is evaluated against the graph locks and process observations.
