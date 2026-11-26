# Two-member native workspace qualification

The verifier requires clean checkouts of both Git revisions in the receipt. It builds the pinned framework, checks every manifest and component input, then assembles and compiles the native workspace with upstream files and network access unavailable. After that clean release build succeeds, it injects the original core integration test file and the byte-exact upstream codec and window test modules at native privacy scope. The receipt records all seventeen test names and the test hashes.

```sh
python3 -B verification/whole/workspace/verify.py \
  /path/to/pinned/oddlang /path/to/pinned/marmish /tmp/native-workspace-receipt.json
cmp /tmp/native-workspace-receipt.json verification/whole/workspace/reference-aarch64-apple-darwin.json
```

The native macOS ARM64 and Linux ARM64 receipts have identical workspace, test module and test-name observations. Linux uses a separate network namespace, an allowlist of files exposed with bubblewrap, and a process with zero effective capabilities. Its replay command uses the same inputs as the macOS run. Compare its output with `reference-aarch64-unknown-linux-gnu.json`. Individual member releases also carry independent public caller and mutation evidence.

## Three-member workspace

`oddlang/portable/workspace-v2.json` pins the native core, stream and command-line packages. The stream library exposes the codec, windows, copy Launch and canonical wire interfaces together. The command-line package provides the two CLI parsers and the explicit-socket status client.

```sh
oddlang build native-workspace rust oddlang/portable/workspace-v2.json /tmp/oddlang-marmish-build
cargo +1.97.1 build --workspace --locked --release --offline --manifest-path /tmp/oddlang-marmish-build/Cargo.toml
python3 -B verification/whole/workspace/verify.py /path/to/pinned/oddlang /path/to/original /tmp/v2.json --release v2
```

Use a fresh destination. Provision the pinned Rust toolchain and prepopulate the registry cache for every dependency in the lock, including target-specific dependencies used by Cargo metadata. The verifier compares staged inputs, generated source inventories and locks before a release build with the original sources and network inaccessible. Then it injects the unchanged core, codec, windows and wire suites and checks all 25 results. The wire test caller supplies four private bounds referenced by upstream tests. They are test inputs. The release build has already finished before the verifier adds them.

The native crate currently has CLI and local status projections. The generated daemon parser, command processing, relay and installed runtime have separate obligations. The core and stream libraries in this workspace can also be consumed independently.

## Workspace v3

Build from this companion checkout with Oddlang revision `11a8b7038b37295729b9e43cfe9a917b9a688f14`, a populated Cargo dependency cache and a fresh output directory.

```sh
oddlang build native-workspace rust oddlang/portable/workspace-v3.json /tmp/marmish-v3
cargo +1.97.1 build --manifest-path /tmp/marmish-v3/Cargo.toml --workspace --locked --release --offline
```

The product inputs are the release manifest, component graphs, target overlays and locks. The native client supports automatic local status discovery. The stream member adds copy Manifest, Mtime, HelperContext and cross-record validation to the v2 interfaces. Both generated executables match the frozen CLI parser transcript on both ARM64 hosts. Running a remote command or transfer still requires further implementation.

For differential qualification, provide clean compiler and upstream checkouts and the seven original executables for the host. Generation and the release build run with upstream source and network access excluded. The verifier runs 25 unchanged upstream library tests and the unchanged native process-limits integration test. The latter stages the two workspace executables and detects replacing either one. It also replays all 34 frozen parser cases against each role in both the original release and the generated workspace, compares 94 copy relation cases, and exercises 43 status discovery controls plus two live discovery routes against an original daemon.

```sh
python3 -B verification/whole/workspace/verify.py /path/to/pinned/oddlang /path/to/pinned/marmish /tmp/v3.json --release v3 --original-bin-dir /path/to/original/release
```

The [macOS ARM64](v3-aarch64-apple-darwin.json) and [Linux ARM64](v3-aarch64-unknown-linux-gnu.json) receipts record the same workspace lock, upstream test identities and copy relation observations. Native binary hashes identify the individual builds.

## Workspace v4

The [v4 manifest](../../../oddlang/portable/workspace-v4.json) combines the v3 core and stream libraries, the native discovery client, and the foreground daemon under one locked Cargo release. Use the pinned Oddlang revision `68dc2e327aae8242938df6379cf5568c39190d65`, the pinned original checkout and native original executables.

```sh
python3 -B verification/whole/workspace/verify.py FRAMEWORK ORIGINAL RECEIPT --release v4 --original-bin-dir ORIGINAL_BINS
```

The verifier generates and builds with source, oracle and network access excluded. It then runs 25 unchanged upstream core and stream tests and one unchanged process-limits test, including per-role controls for the latter. The upstream process test checks that each binary returns help. On macOS it first confirms the child inherited a descriptor soft limit of 65,537. On Linux it exercises the help path. The verifier also replays 68 parser cases across the two binary roles and checks the copy field relations and discovery controls. The foreground matrix runs both original and generated clients against each real daemon. The daemons use an originally prepared private file profile, separate held loopback TLS listeners and ordinary public CLI names. The checks cover auto-discovered activation status, plaintext and trust errors, cross-implementation leases, SIGTERM, reattach and crash recovery. The scope ends at direct TLS startup and local status. Provider readiness, execution and installed service support remain open.

On Linux, prewarm a writable offline Cargo registry with `cargo +1.97.1 metadata --format-version 1 --locked --offline` on a generated workspace before the gate mounts that registry read-only. Cargo may need to unpack target-specific packages even when their archives are cached.

The [macOS ARM64 receipt](v4-aarch64-apple-darwin.json) and [Linux ARM64 receipt](v4-aarch64-unknown-linux-gnu.json) identify the exact verifier revision and generated executables. Both report the same workspace locks, parser transcript, library tests and foreground outcomes. The pre-existing no-home discovery case retains its platform-specific result. Linux uses the configured XDG state directory, and macOS reports `home_directory`.

## Workspace v5

The [v5 manifest](../../../oddlang/portable/workspace-v5.json) builds the core library, stream library with the complete copy protocol codec, and the native executables with encrypted file-profile initialization and foreground local status. Use Oddlang revision `fa40d7fab0bd6308ed7b13ef2aa97206145ebd36`, Rust 1.97.1 and a populated offline Cargo cache.

```sh
oddlang build native-workspace rust oddlang/portable/workspace-v5.json /tmp/marmish-v5
cargo +1.97.1 build --manifest-path /tmp/marmish-v5/Cargo.toml --workspace --locked --release --offline

python3 -B verification/whole/workspace/verify.py FRAMEWORK ORIGINAL RECEIPT --release v5 --original-bin-dir ORIGINAL_BINS
```

Prepare the registry cache with the locked metadata command above for v4, using the v5 workspace. The v5 gate builds four fresh shallow Git repositories from the provider commits already in Cargo's local Git cache. It stages those commits and the registry cache, then generates and builds with the original application source and binaries, independent fixtures, and network excluded. The release lock binds the generated files and native dependency set.

The verifier runs all 29 unchanged upstream stream tests, seven upstream core tests, nine independent exact frame checks, and the unchanged executable help smoke test. It also repeats the 68 parser cases, 94 copy field relations and status discovery probes. The foreground matrix initializes two fresh named profiles, one with each client. Each client then discovers and queries each daemon while a real loopback TLS attempt is held. The matrix checks lease contention, TLS trust failure, shutdown, crash recovery and preservation of the initialized identity and credentials. A separate observer opens both encrypted databases and checks the schema and empty-state counts before a cross-consumer can repair either profile.

This release covers explicit direct-relay initialization for fresh and pristine file-backed profiles, local status during foreground activation, and the in-process copy codec. Provider readiness, older or nonempty profile migrations, Keychain, Tor, installed services, end-to-end transfers and remote command execution remain open.

The [macOS ARM64 receipt](v5-aarch64-apple-darwin.json) and [Linux ARM64 receipt](v5-aarch64-unknown-linux-gnu.json) agree on the generated locks, profile/foreground observations, database shapes, stream tests and exact frame checks. Native artifact hashes identify each host build. The no-home discovery case retains the original platform-specific result. Linux also records matching original and generated SQLCipher diagnostics for the corrupt-journal test.

### Bootstrap v5 with an empty Cargo cache

Use Oddlang revision `e2ec7338dcf141f95d2f1e24d9bee20cc1cec44b` from [the dependency bootstrap PR](https://github.com/Epiphytic/oddlang/pull/63) and select Rust 1.97.1 for the generated workspace. The first command can contact the registries and Git providers pinned by the release. The second command builds both executables offline. You can copy `oddlang/portable` into a fresh directory to run both commands outside this checkout.

```sh
export CARGO_HOME="$(mktemp -d)"
RUSTUP_TOOLCHAIN=1.97.1 oddlang build native-workspace rust oddlang/portable/workspace-v5.json /tmp/marmish-fresh --fetch-locked
cargo +1.97.1 build --manifest-path /tmp/marmish-fresh/Cargo.toml --workspace --locked --release --offline
```

The [cold-cache verifier](cold_cache.py) stages only the 55 inputs reachable from the v5 manifest. In a new Cargo home it checks the default offline failure, permits locked fetching in a second empty home, regenerates with Cargo offline and compares both products to the published workspace, package and source hashes. It runs the frozen 34-case parser transcript on each release executable. It accepts a clean pinned Oddlang checkout and a receipt output path. Building the verifier’s compiler uses its own existing toolchain dependency cache.

The [macOS ARM64 receipt](cold-cache-aarch64-apple-darwin.json) and [Linux ARM64 receipt](cold-cache-aarch64-unknown-linux-gnu.json) record identical input digests and workspace lock. Each host compiled both release binaries offline after acquisition and passed 34 parser cases per binary.

## Workspace v6

The [v6 manifest](../../../oddlang/portable/workspace-v6.json) builds the same v5 core and stream libraries with both qualified file-profile roles. The native client initializes encrypted state using explicit or supported stored configuration. The same native release supplies the foreground provider owner and discovering status client for prepared no-config file profiles during direct loopback TLS activation. The v5 release and both its library member locks remain intact.

Use Oddlang `ab7171bb583f7e04c63896caeafaa476eb810d5a`, Rust 1.97.1, and the completed offline cache described in the v5 instructions. Use a fresh destination.

```sh
oddlang build native-workspace rust oddlang/portable/workspace-v6.json /tmp/marmish-v6
cargo +1.97.1 build --manifest-path /tmp/marmish-v6/Cargo.toml --workspace --locked --release --offline
python3 -B verification/whole/workspace/verify.py FRAMEWORK ORIGINAL RECEIPT --release v6 --original-bin-dir ORIGINAL_BINS
```

Generation and the release build run with application source, original executable and oracle access excluded, and with network access denied. All native observers use the same generated executable pair. The configured-init observer checks filesystem selection, checked configuration and bidirectional reuse of encrypted current-format recent journal records. The foreground observer crosses both original and generated initializers, owners and clients with direct, no-config profiles. It checks active store ownership while TLS is held, original-matching signal and trust-error reports, startup rejection, and usable durable state after shutdown. The v5 parser, stream, core, discovery, copy relation and abrupt ownership recovery cases are replayed on the same release.

Configured foreground execution, readiness beyond TLS, older or nonempty migrations beyond the configured initializer's named cases, Keychain, Tor, installed services and end-to-end transfers remain open.
