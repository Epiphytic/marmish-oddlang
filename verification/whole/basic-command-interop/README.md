# Basic-command reference control

Run on macOS ARM64 with the original binaries identified by `../bridge/aarch64-apple-darwin.json`. The pinned Deaddrop binary and the original `installed-checks` fixture have separate provenance in `reference-build-macos-arm64.json`. Pass their recorded binary digests. The focused macOS control executes the Command Line Tools Python framework interpreter directly. Its digest is pinned in `short-fixture-runtime-macos-arm64.json`. This avoids both the Xcode discovery launcher and the separate Python framework launcher.

```sh
python3 -B -W error::ResourceWarning -m unittest discover -s verification/whole/basic-command-interop
python3 -B verification/whole/basic-command-interop/replay_corpus.py \
  /path/to/marmish-installed-fixture FIXTURE_SHA256
python3 -B verification/whole/relay/owner_tls.py \
  /path/to/original/bin verification/whole/bridge/aarch64-apple-darwin.json /tmp/owner-tls.json
cmp /tmp/owner-tls.json verification/whole/relay/owner-tls-macos-arm64.json
python3 -B verification/whole/basic-command-interop/observe.py \
  /path/to/original/bin /path/to/deaddrop RELAY_SHA256 \
  /path/to/marmish-installed-fixture FIXTURE_SHA256 \
  /Library/Developer/CommandLineTools/Library/Frameworks/Python3.framework/Versions/3.9/Resources/Python.app/Contents/MacOS/Python
```

The observer provisions fresh private file-backed credentials and uses a fresh test-owned Deaddrop onion relay. The relay and peer clients need public Tor transit. All application messages go to the owned relay through its private onion address. The runner's JSON receipt publishes only sanitized build and result evidence. The direct corpus receipt qualifies fixture byte preservation and server receipt formulas. A blocked transport or command attempt is evidence for its stage only.

Add `--generated-dir /path/to/export --generated-role server` for upstream client to generated server, or `--generated-role client` for the reverse pairing. The export's isolated-build receipt binds its executables. The harness verifies each shipped executable selected for that side and records missing generated artifacts without copying any upstream executable into that side's workspace.
