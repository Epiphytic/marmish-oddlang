# Basic-command reference control

Run on macOS ARM64 with the original binaries identified by `../bridge/aarch64-apple-darwin.json`. The pinned Deaddrop binary and the original `installed-checks` fixture have separate provenance in `reference-build-macos-arm64.json`. Pass their recorded binary digests. The focused macOS control executes the Command Line Tools Python framework interpreter directly. Its digest is pinned in `short-fixture-runtime-macos-arm64.json`. This avoids both the Xcode discovery launcher and the separate Python framework launcher.

```sh
cargo +1.97.1 test --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin check-command-output --bin replay-command-corpus
python3 -B -W error::ResourceWarning -m unittest discover -s verification/whole/basic-command-interop
cargo +1.97.1 run --quiet --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin replay-command-corpus -- \
  /path/to/marmish-installed-fixture FIXTURE_SHA256
python3 -B verification/whole/relay/owner_tls.py \
  /path/to/original/bin verification/whole/bridge/aarch64-apple-darwin.json /tmp/owner-tls.json
cmp /tmp/owner-tls.json verification/whole/relay/owner-tls-macos-arm64.json
python3 -B verification/whole/basic-command-interop/observe.py \
  /path/to/original/bin /path/to/deaddrop RELAY_SHA256 \
  /path/to/marmish-installed-fixture FIXTURE_SHA256 \
  /Library/Developer/CommandLineTools/Library/Frameworks/Python3.framework/Versions/3.9/Resources/Python.app/Contents/MacOS/Python
```

The direct Rust replay builds its short fixture offline and compares every exit status, both output streams and the private receiver receipt. Pass `--historical-short` to run the pinned Python fixture and reproduce `direct-corpus-macos-arm64.json` byte for byte. The live reference observer retains its original fixture and interpreter pins.

The observer provisions fresh private file-backed credentials and uses a fresh test-owned Deaddrop onion relay. The relay and peer clients need public Tor transit. All application messages go to the owned relay through its private onion address. The runner's JSON receipt publishes only sanitized build and result evidence. The direct corpus receipt qualifies fixture byte preservation and server receipt formulas. A blocked transport or command attempt is evidence for its stage only.

Add `--generated-dir /path/to/export --generated-role server` for upstream client to generated server, or `--generated-role client` for the reverse pairing. The export's isolated-build receipt binds its executables. The harness verifies each shipped executable selected for that side and records missing generated artifacts without copying any upstream executable into that side's workspace.

For a newly built candidate, pass its independently pinned `--generated-build-record` and `--generated-preflight-record`. The first provides the expected artifact manifest. The second binds the export's receipt digest. The output records both pin digests. Omitting these options selects the historical build and preflight records in this directory.

Use `--activation-only` with a mixed pairing to observe setup, peer authorization and relay activation. The upstream owner reports `active` through its CLI. The generated owner reports `provider_active` over its own discovered private socket. The observer checks each account's signed key package in the owned relay database and records scoped activation evidence and cleanup.

Replay the sealed setup candidate using `--generated-build-record verification/whole/basic-command-interop/profile-setup-initial-build-macos-arm64.json` and `--generated-preflight-record verification/whole/basic-command-interop/profile-setup-initial-preflight-macos-arm64.json`. The associated mixed receipts identify the generated role and the observed owner exit.

The macOS process-policy diagnostic runs three pinned binaries against isolated loopback endpoints and queries each live process's kernel policy. Run `python3 -B verification/whole/basic-command-interop/audit_mac_policy.py diagnostic /path/to/original/bin /path/to/deaddrop` for the bounded diagnostic. Its receipt records executable digests and allowed or denied operations without credential contents, process arguments or host file paths.

Use `--first-foreground-only` for a fresh mixed attempt with the fixed `success` case. It checks the expected output and exit status and separately records whether the intended receiver published its fixture receipt. This scope measures the first foreground command once per selected pairing.

The observer samples the existing host preferences and credential metadata at stage boundaries, before each command timer and after teardown. If a sample changes, the receipt lists only the interval labels and affected categories. A later restoration still leaves that observation in the receipt. The interval narrows the time of a change. It cannot identify which process made it. File names, contents and timestamps stay out of the receipt.

## Qualified reference run

The [macOS ARM64 reference receipt](upstream-qualified-corpus-macos-arm64.json) records all 24 matched and attributed cases on the pinned mainline pair. Each finite case returned status 7 and delivered 131,072 bytes on each stream. Measured whole-operation goodput was 3,698 bytes/s cold and 3,818, 3,815 and 3,836 bytes/s warm. The observer verified the two distinct identities, private file credentials and authorizations, removed all workspaces, and found no host metadata changes or audit errors. The run used a working tree whose observer scripts were committed as `2e90c7fe3a6e6ed9f559cc70043ecf5586e41fa3`. The candidate directions require their own receipts.

An [earlier attempt](upstream-engine-startup-deadline-macos-arm64.json) stopped at the historical 20-second engine startup check before any command. Teardown passed. The qualifying run shared the same total 190-second upstream-server activation budget between database readiness and daemon readiness, and polled for process exit during database initialization. Earlier blocked runs retain their original results.

With `--first-foreground-only`, omit the generated peer options to run the
same single case on two mainline peers. A successful control has outcome
`control-foreground-pass`. Full reference qualification still requires the
entire corpus. The foreground observer reads the test relay's SQLite event
counts by numeric kind immediately before and after the case, including when
the output comparison fails. Counts measure rows present at each snapshot.
They cannot attribute an event to a command, prove delivery to the receiver,
or account for rows inserted and deleted between samples. The designated
server's fixture receipt remains the execution check.
