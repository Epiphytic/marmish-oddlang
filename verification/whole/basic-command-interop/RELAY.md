# Generated foreground relay provider

The [relay activation workspace](../../../oddlang/portable/workspace-relay-activation.json) adds an explicit onion route to the generated foreground owner. It retains the command request projections from the basic-command workspace and gives the local status owner a separate `provider_active` transition. The companion graph selects one direct input URL through Tor, private Arti directories, a WebSocket frame bound and the application component IDs observed in the original startup. The Rust binding selects the pinned Arti, Nostr and Marmot providers.

The daemon opens its retained encrypted stores under the profile lease before network bootstrap. It uses the canonical relay endpoint for both transport and authentication, publishes the durable signed KeyPackage and activates account transport before exposing `provider_active`. This phase covers the provider handshake. Command admission and execution remain separate.

## Replay

Build with `build.py --workspace workspace-relay-activation.json` and the exact companion and framework commits supplied as `--companion-revision` and `--framework-revision`. Supply the original source, original executable and oracle read-denial probes from [BUILD.md](BUILD.md). The driver stages the portable closure, validates the pinned workspace output and compiles in the existing source-denied and network-denied sandbox.

Pass the exported `marmish` and `marmishd`, the pinned Deaddrop executable, the oracle's `isolate.py` and an output receipt path to `provider_activation.py`. Supply each binary digest as the corresponding SHA-256 option. The fixture initializes a fresh file profile with an inert loopback configuration, then supplies the actual test-owned onion explicitly to both foreground startups. It queries the test-owned relay's database read-only to check the signed KeyPackage and verifies identical publication bytes after restart. It records only sanitized outcomes and artifact digests.
