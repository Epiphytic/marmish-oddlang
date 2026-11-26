# Client and daemon CLI parser baseline

The transcript freezes 34 invocations of each executable under an empty disposable home and the C locale. The driver discovers the advertised commands from root help, probes help for each, adds the four hidden evidence/cleanup commands, and exercises version and an unknown global option. Each row pins arguments, exit status and both output channels. Changing a help exit status in the fixture is detected. This is a parser baseline. The remote-destination syntax, internal copy helper and stateful operation behavior have separate obligations.

Run `python3 replay.py verify /path/to/installed/bin` with independently staged release binaries. Capturing a new fixture requires the original binaries matching the selected build receipt. Ports, credentials and live services are unnecessary for this check.


## Generated parser-only executables

The [native CLI candidate](../../../oddlang/portable/native-cli-v1/) supplies the graph, executable overlays and workspace pins. Generate and replay both binaries on macOS with:

```sh
verification/whole/cli/macos.sh /path/to/oddlang /path/to/pinned/original /path/to/new/build
```

The recovery step reads the pinned original parser from Git. The workspace generation and locked offline release build run with network access denied and the original checkout, original Git objects and frozen transcript unreadable. Replay gets the transcript only after generation and build. The standalone `project.py` accepts separate `recover` and `generate` runs, followed by the existing `replay.py verify` command.

The replay qualifies the parser slice for both executable roles. Stateful commands and daemon IPC are uncovered. The [macOS ARM64 receipt](generated-aarch64-apple-darwin.json) records the exact Oddlang revision, toolchain, locks and generated binary hashes.
