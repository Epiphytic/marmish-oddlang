# Pinned upstream test suite

The release reference is upstream Marmish commit
`af54339d250290490d62d4475a77539b7e739071`. Run these verification commands
against an independent checkout of that commit. The upstream checkout is an
oracle input and belongs outside the source-free candidate build.

```sh
python3 verification/whole/suite/inventory.py "$UPSTREAM" > /tmp/marmish-upstream-tests.json
cmp verification/whole/suite/inventory.json /tmp/marmish-upstream-tests.json
```

The inventory reads the pinned Git objects and records file identities. It lists
130 Rust files with unit-test attribute markers, ten Rust integration targets and
their six support files, 54 Python test modules, 21 Node test modules, two C fixture
test sources and 27 workflow definitions. Rust attribute markers identify source
files for audit. For each actual Cargo profile, obtain the executable target list
with locked Cargo metadata, run the test harnesses with `--list`, and preserve
separate ignored and platform-filtered dispositions. A source file count cannot
stand in for that enumeration. Integration rows with direct `env!` binary
references name those bindings. A target can also acquire binaries through its
support files.

## Running the reference tests

Use a writable, disposable upstream checkout. Some tests require Git metadata,
compile auxiliary native fixtures or write to build directories. Use a canonical
`TMPDIR` on Darwin, a Python build that exposes the required OS APIs including
`os.waitid`, and the interpreter versions of the selected workflow. The
report in this directory describes the actual local runs.

| Family | Original entry point | Requirements and subject |
| --- | --- | --- |
| Rust unit, integration and doctest | `cargo test --locked --release --workspace --no-fail-fast -- --test-threads=1` | Builds the workspace and default products under test. Select default dev separately by removing `--release`. |
| Rust fault-injection | `cargo test --locked --release -p marmish-engine --features fault-injection --test faults -- --test-threads=1` | Enables the gated test frontend. Keep distinct from the default profile. |
| Rust installed checks | `cargo test --locked --release -p marmish-native --features installed-checks -- --test-threads=1` | Compiles installed-check modules. Run installed acceptance separately against staged artifacts. |
| Python tools, keys and preflight | `python3 -m unittest discover -s tools`, `PYTHONPATH=tools python3 -m unittest discover -s tests`, `python3 -m unittest discover -s tests/ci` | Install Python dependencies used by the workflow. Run each discovery root explicitly. |
| Python ontology tooling | `sh tools/code-ontology/test.sh` | Fetches a pinned schema, verifies its SHA-256 and uses the separately locked Python 3.12 environment. |
| Node Actions tooling | `npm ci --prefix tools/v02-actions --ignore-scripts` and `npm test --prefix tools/v02-actions` | Lockfile dependencies, Node 24 Actions runtime and Git history for policy cases. |
| Node macOS engine probe | `node --test tools/macos-engine-inherited-limits/probe.test.mjs` | Exercises probe logic. The OS observation is a separate action. |
| C lifetime fixtures | Python discovery includes `tools/test_marmish_lifetime.py`. | The Python wrapper compiles C fixtures. Real installed lifetime handling needs product-level acceptance. |
| Workflow, installed and live acceptance | Use the pinned workflow definitions to locate the selected job arguments and prerequisites. | Requires applicable platform, approved relay/peers, release assets or installed tree, and OS providers. Preserve results and omissions per job. |

The [Darwin interpreter receipt](interpreted-aarch64-apple-darwin.json) records
979 passing Python tool tests and 45 platform or prerequisite skips. The key,
preflight, ontology and Node probe suites passed. Using a clean Git checkout and
a canonical temporary directory mattered for Git-backed fixtures and Darwin
path and permission checks. The Node 24 Actions suite passed 261 of 263 tests.
Two pending-promise cases reached the 20-second per-test bound. Full Node
baseline closure remains open. These are local suite results. Installed
acceptance and live provider workflows have separate prerequisites.

## Binding candidate products

| Suite or test family | Candidate binding and evidence required |
| --- | --- |
| Rust unit tests and private in-tree tests | Cargo builds the crate being tested. For reuse, compile test code in a verifier copy against the candidate source or testable implementation of that crate. Private helpers, `#[cfg(test)]` features and self-exec of the test runner can require explicit test seams. A test runner that invokes its own executable exercises the code linked into that runner. |
| Rust public API integration and doctests | Extract unchanged consumer assertions into a verifier crate that depends on the candidate package under the same crate identity and features. Bind every workspace dependency participating in the assertion to its candidate or independently pinned peer role. Verify Cargo metadata and rustc paths. |
| Cargo-bound native processes | Compile-time `CARGO_BIN_EXE_*` binds process artifacts in engine storage/worker cases, fault frontend cases, the bridge integration target, native client/daemon help and vault helper cases. Recompile the verifier with explicit paths, using an isolated verifier build, or add a reviewed runtime selector with no fallback. Running `cargo test` in the original tree normally builds and selects its own products. |
| Engine worker fixtures | The engine support module requires the incoming name `marmish-engine-worker` and copies it into a private install directory as `marmish-worker`. Record both hashes and keep the native and engine-worker slots distinct. |
| Native-vault and copy helper tests | Source includes subprocesses that launch the test executable itself and platform-specific provider fixtures. Check each case for a test harness, a staged product, or both. A substituted executable alone cannot establish candidate behavior for the linked component. |
| Legacy Python and Python fixture tests | Python unittest imports modules from the upstream checkout. For a product role being replaced, load the candidate implementation in a clean verifier environment or preserve the Python test as a reference case that calls the candidate externally. Test fixtures that compile auxiliary binaries, such as `test_marmish_installed_fixture.py`, need an explicit compiler allowlist or an independently pinned fixture artifact. |
| Explicit executable arguments | `tools/marmish_e2e.py` accepts `--bridge`, `--worker`, `--wnd` with digest, and an optional installed root and keychain helper. `tools/marmish_native_validation.py` accepts `--helper`. Bind each supplied path and record which executables each case actually starts. |
| Archive, installed tree and provider acceptance | `tools/marmish_release_acceptance.py` takes `--archive` and a disposable `--prefix`. Native quickstart and provider checks take `--bootstrap` and `--assets-dir`. Bind archive, manifest, checksums and installer to the selected variant. Check resolved child executables after installation and replacement, including provider and installed-check helpers. |
| Node tests and operator workflows | Unit tests import Node modules and often use fake tool paths. For installed workflow evidence, inspect resolved tooling, artifact pins, launcher paths, peers and any build steps. A passing coordinator or policy test establishes the workflow assertion it runs. |

Every candidate execution receipt should identify the upstream test or scenario,
target and profile, selection mechanism, binaries or linked libraries exercised,
digests and actual terminal result. Verify that fixture and peer builds can only
read their declared inputs. Withhold original product sources and executables
from candidate test cells except for explicit mixed-peer roles. Audit descendants
to catch accidental fallback, and require a missing or substituted candidate
artifact to fail in a deliberate control run. A test that merely builds the
original implementation remains original baseline evidence. Test selection and
expected outcomes come from the pinned suite and independently authored cases.

## Verified external executable selection

`process_limits.py` extracts the unchanged upstream native-process integration
test and compiles just that verifier with `rustc --test`, a matching third-party
`libc` artifact and compile-time paths to explicitly staged client and daemon
executables. The verifier invokes rustc directly. The optional Cargo harness is a negative
control for attempts to use runtime `CARGO_BIN_EXE_*` overrides.

```sh
python3 verification/whole/suite/process_limits.py "$UPSTREAM" \
  --rustc "$RUSTC" --libc "$LIBC_RLIB" \
  --client "$CLIENT" --daemon "$DAEMON" --cargo-harness "$ORIGINAL_TEST_HARNESS"
```

The Darwin receipt uses original release binaries. Both baseline artifacts pass
under the inherited-limit test. Replacing either staged executable with a shell
stub that exits successfully without help text fails the upstream assertion. The
original Cargo harness still passes with runtime overrides pointing to a missing
file. On Darwin the upstream harness requires a hard descriptor limit greater
than 65,536. This case establishes artifact selection and help smoke coverage.
Candidate executions and Linux cells are open.

The full original release workspace suite on macOS 26.6 exposed an additional baseline failure. When the MDK tests ran serially in one process, the first empty-environment test failed and five subsequent tests reported a poisoned shared mutex. In fresh processes eight of the nine passed. The remaining test copies /usr/bin/env before executing it. In an independent control, the installed executable exited 0 under an empty environment and its temporary copy received SIGKILL. The cause of that OS signal is unverified. Run `mdk_host.py` with the original MDK unit-test binary to reproduce the distinction. [Fresh-process observations](mdk-aarch64-apple-darwin.json)

## Named release tests

Rebuild the pinned original in a private directory with a prepopulated Cargo registry, then list its test harnesses. For the subsequent execution run, set umask 077 before creating the checkout and build directories. The engine exercises path-permission checks on its working directory and executable.

```sh
python3 -B verification/whole/suite/list_cargo.py "$UPSTREAM" "$CARGO_TARGET_DIR" /tmp/marmish-release-tests.json
```

Each row identifies a Cargo workspace package, target kind and target name, source path, the package’s own features and executable digest. The two harness listings distinguish default runnable cases from ignored cases. The inventory covers compiled host-default release harnesses. Doctests, fault-injection builds, installed checks and interpreted suites have separate entry points. Harness digests identify a particular build and may vary across build directories.

The [macOS ARM64 inventory](named-release-aarch64-apple-darwin.json) contains 909 runnable cases. The [Linux ARM64 inventory](named-release-aarch64-unknown-linux-gnu.json) contains 887. Each lists 24 compiled harnesses and five ignored cases. Some ignored cases are child-process entry points invoked by other tests. These counts describe test discovery. Case execution results are recorded separately.

## Complete original default release on Linux ARM64

With the checkout and target directory under a private home directory, the unchanged original passed all 887 runnable cases. All 24 compiled harnesses and seven library doctest targets returned success. Five cases were ignored by the default harness selection. The [execution receipt](private-release-aarch64-unknown-linux-gnu.json) binds the completed Cargo log to the [named inventory](named-release-aarch64-unknown-linux-gnu.json) through the exact 24 executable digests. It records original behavior for later candidate comparison.

The earlier public-directory run remains as evidence for `UnsafePath`. This successful run used a fresh clone under a mode-0700 directory with umask 077 set before checkout, build and test. The package working directory and test executables thus both satisfied upstream’s path checks.

```sh
umask 077
root=$(mktemp -d "$HOME/marmish-baseline.XXXXXX")
git clone --no-local "$UPSTREAM" "$root/reference"
git -C "$root/reference" checkout --detach af54339d250290490d62d4475a77539b7e739071
mkdir "$root/build"
(cd "$root/reference" && CARGO_TARGET_DIR="$root/build" cargo +1.97.1 test --locked --offline --release --workspace --no-fail-fast -- --test-threads=1) > "$root/test.log" 2>&1 &&
python3 -B verification/whole/suite/list_cargo.py "$root/reference" "$root/build" "$root/named.json" &&
python3 -B verification/whole/suite/summarize_cargo.py "$root/test.log" "$root/named.json" "$root/result.json"
```

## Upstream tests on the five-crate generated candidate

The runner measures the v7 vault/engine workspace on each native host. It seals the generated package files, builds the release client and daemon, and then stages the upstream test modules. Each host has its own case-by-case receipt. For this original v7 run, each of the three selected harnesses must pass in full and match the catalog. The later recovered workspace also qualifies explicitly selected cases within two additional harnesses.

| Upstream harness | Cases | How it binds to the candidate |
| --- | ---: | --- |
| Core contracts | 7 | Unchanged upstream integration test compiled against generated core |
| Stream library | 29 | Upstream test modules compiled against generated stream. Test-only wrappers supply private imports and constants for the wire and copy callers |
| Native process limits | 1 | Unchanged upstream test recompiled against the two built executable paths. Substituting either role must fail the upstream help-output assertion |

The process test runs outside the build sandbox. Product-specific assertions and tests on other workspace versions earn no credit here. The catalog covers native default-feature Rust release harnesses. Doctests, feature-specific builds, Python, Node and live acceptance runs require separate evidence.

To reproduce a receipt, use a clean checkout of each recorded revision, a populated offline dependency cache and the host’s native toolchain. The four directory arguments identify the upstream checkout, Oddlang checkout, Oddlang compiler build cache and original Cargo target, respectively.

```sh
python3 -B verification/whole/suite/candidate_coverage.py \
  "$UPSTREAM" "$ODDLANG" "$COMPILER_CACHE" "$ORIGINAL_TARGET" /tmp/candidate.json
python3 -B verification/whole/suite/check_coverage.py /tmp/candidate.json --require-complete
```

The completeness check exits 1 while any eligible catalog case lacks passing evidence.

Most of the remaining default-release cases belong to the native and engine crates. The following v7 table counts runnable tests from the pinned upstream catalogs. The last column counts cases exercised by this runner on each host.

| Upstream crate | macOS ARM64 catalog | Linux ARM64 catalog | Qualified per host |
| --- | ---: | ---: | ---: |
| marmish-native | 645 | 646 | 1 |
| marmish-engine | 193 | 170 | 0 |
| marmish-stream | 29 | 29 | 29 |
| marmish-vault | 15 | 15 | 0 |
| marmish-prototype | 11 | 11 | 0 |
| marmish-mdk | 9 | 9 | 0 |
| marmish-core | 7 | 7 | 7 |

The [macOS candidate receipt](combined-v7-aarch64-apple-darwin.json) records 37 passed and 872 unverified. The [Linux candidate receipt](combined-v7-aarch64-unknown-linux-gnu.json) records 37 passed and 850 unverified. Each catalog also has five ignored cases. These counts belong to workspace lock `c07772ff1fd78ea0ea166a17cf1b9a61f6ad00823dea81d1fe0d681c49b34df3`.

## Workspace with recovered release metadata and vault values

Use `--candidate recovered` on the same candidate runner with Oddlang revision `6e2d1ef00f286b6132b84e27a61805c659dfd437`. This builds the [five-member recovered workspace](../../../oddlang/portable/workspace-v7-recovered-vault-engine.json) from a fresh directory, then reruns core, stream and native process cases. Two additional upstream library harnesses have partial coverage on this candidate.

- The original vault module tests construction, width and checked input through the generated crate’s public types. Its original test name is `tests::strict_public_values`.
- The engine harness includes original test functions and private test helpers. It runs each limit change in an isolated child process and checks that the soft and hard limits and an existing descriptor’s flags survive two calls. The Linux-only case checks that initialization preserves the process limits.
- The vault and engine functions run as Cargo integration tests against their generated libraries. The runner preserves their upstream module paths so the engine’s self-invocation selects the same test. It checks complete selected name sets, pins the digests of the original test bytes, and compares those names with each host’s complete catalog. Other tests in those two upstream harnesses remain individually unverified.

The original workspace is needed by the test oracle. Both generated release executables are built before the test sources are staged.

The credited engine cases cover preserved limits. A no-op implementation also passes those cases. High-soft-limit normalization on macOS and the rest of the engine library need their own evidence.

The [macOS receipt](recovered-aarch64-apple-darwin.json) records 39 passed and 870 unverified. The [Linux receipt](recovered-aarch64-unknown-linux-gnu.json) records 40 passed and 847 unverified. Each has five ignored. Both cold runs used companion verifier commit `64f86802a4348fe424977bf134f4ecb45daa4f60`. An engine case gets one catalog credit after its parent and child both run that exact test. Changing the child's filter to an absent name lets Cargo return success, while the coverage gate rejects it on both hosts. [Child-selection control](recovered-child-control.json)

## Seven-member workspace

Use `--candidate seven` with compiler `59f947dafdb4cf3055e35216857472ada895a76a` to run this selection on the [seven-member workspace](../../../oddlang/portable/workspace-v7-recovered-vault-engine-journal-mdk-buffered.json). Before staging any tests, Cargo must produce all seven sealed libraries and both declared release executables. The receipt records each library artifact digest and binds the process test to both executable digests.

Core contracts, stream tests, native process limits, vault values and the engine's selected limits cases rerun on that build. The single original prototype case, `secret_request_tests::private_reader_wipes_consumed_spans`, is staged unchanged into the generated private buffer module. Its qualified case count is one. The other prototype unit case, all bridge integration cases and all original MDK cases remain unverified by this run. The MDK library's presence means Cargo produced its artifact. Public API and behavior compatibility require separate evidence.

The [macOS receipt](seven-aarch64-apple-darwin.json) records 40 passed and 869 unverified. The [Linux receipt](seven-aarch64-unknown-linux-gnu.json) records 41 passed and 846 unverified. Each host has five additional ignored tests. Both hosts ran verifier commit `283d1c5783000266942e0312e2711680708ee001` against the companion workspace input `6a3eb779b60e63bef3dbfbb83e64b8a486ef77df` and workspace lock `fdfa5b16d9172fe97705ee5794251c6168d0e226ea159334a52286e2e7172b68`.
