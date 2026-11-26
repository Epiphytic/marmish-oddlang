# Generated foreground relay provider

The [relay activation workspace](../../../oddlang/portable/workspace-relay-activation.json) adds an explicit onion route to the generated foreground owner. It retains the command request projections from the basic-command workspace and gives the local status owner a separate `provider_active` transition. The companion graph selects one direct input URL through Tor, private Arti directories, a WebSocket frame bound and the application component IDs observed in the original startup. The Rust binding selects the pinned Arti, Nostr and Marmot providers.

The daemon opens its retained encrypted stores under the profile lease before network bootstrap. It uses the canonical relay endpoint for both transport and authentication, publishes the durable signed KeyPackage and activates account transport before exposing `provider_active`. This phase covers the provider handshake. Command admission and execution remain separate.

## Replay

Build with `build.py --workspace workspace-relay-activation.json` and the exact companion and framework commits supplied as `--companion-revision` and `--framework-revision`. Supply the original source, original executable and oracle read-denial probes from [BUILD.md](BUILD.md). The driver stages the portable closure, validates the pinned workspace output and compiles in the existing source-denied and network-denied sandbox.

Pass the exported `marmish` and `marmishd`, the pinned Deaddrop executable, the oracle's `isolate.py` and an output receipt path to `provider_activation.py`. Supply each binary digest as the corresponding SHA-256 option. The fixture initializes a fresh file profile with an inert loopback configuration, then supplies the actual test-owned onion explicitly to both foreground startups. It queries the test-owned relay's database read-only to check the signed KeyPackage and verifies identical publication bytes after restart. It records only sanitized outcomes and artifact digests.

## Qualification pins

The [build receipt](generated-provider-build-aarch64-apple-darwin.json) binds framework commit `01764f5ebc66b57402e59d680c1d771b56884db2` and companion input commit `da58dcc14dae64c7d65bff9b4c759f0dab8607d8`. The macOS ARM64 build used 661 checksum-verified registry archives, four pinned Git providers, and seven excluded-file read probes. The isolation helper used for the live fixture comes from companion commit `f33cd9dbaf310852416cb352c741591a597b7c84`. The relay binary is pinned in the live receipt and originates from Deaddrop revision `b38b2841b081952dc7728da391fe0e26fc7c6181`.

The [live receipt](generated-provider-relay-aarch64-apple-darwin.json) binds the exported binary hashes to the owned relay run. The relay stored the signed KeyPackage for the profile identity and retained identical event bytes on restart. The daemon returned its exact shutdown report on signal and exited with the transport failure after relay loss. Both trials removed the private status socket and the harness removed the trial workspace.
