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
