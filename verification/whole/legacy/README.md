# Optional legacy tools

The v0.2+ runtime uses native providers. The qualification lab separately builds an optional reference pair from the public MDK fork at `37c248a11239e108afc9bd4ee62b8f60542012e3`. Its freshly built CLI identifies itself as `wn 0.10.3`. Upstream’s historical installed CLI reported 0.13.0. Those artifacts have different provenance.

To reproduce, check out the MDK fork at this exact revision. Populate its locked Cargo dependency cache. Use the pinned Marmish checkout to run `python3 -B tools/build_mdk.py --output /absolute/fresh/wnd`. This upstream recipe checks the lock, applies maintained platform patches, tests the bootstrap protocol, and on macOS tests transport and verifies the signed binary. Then run the companion observer with the absolute paths to the clean MDK checkout, a dedicated CLI build directory, the prepared wnd, Marmish's original MDK probe example, and its matching bridge build receipt. It builds wn offline and verifies the native probe's exact exit and output with no ambient environment.

The macOS receipt covers offline bootstrap checks and the version observation. Live exchanges with the historical installed pair have their own qualification boundary.

```sh
cargo +1.96.1 run --locked --offline \
    --manifest-path verification/whole/canonical-public-data/Cargo.toml \
    --bin legacy-version-probe -- \
    /path/to/mdk-checkout /path/to/cli-target /path/to/wnd \
    /path/to/original/examples/probe verification/whole/bridge/aarch64-apple-darwin.json
```
