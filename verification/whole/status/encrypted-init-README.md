# Native encrypted profile initialization

The package preserves the existing native discovery-backed status command and
adds the original file-backed init form:

```sh
marmish --state-dir STATE --key-storage file --relay wss://127.0.0.1:9 --tor off init
```

Initialization creates the private profile and two encrypted stores, verifies
the native identity and durable store state, derives counts from the journal,
and changes the profile to ready. It starts no network activation. With these
explicit arguments upstream creates no configuration file. The witness covers
fresh init and reopened pristine current-format stores. Default OS state paths,
persisted relay configuration, Tor, keychain, explicit peers, custom retention,
nonempty journal recovery and migrations across older formats remain open.

Recovery observed upstream revision `af54339d250290490d62d4475a77539b7e739071` at the native CLI, identity lease, storage preflight, journal and account-device boundaries. The graph declares portable layout, startup guards, binding fields, derivation
roles, journal relations and seeds, metrics, and failure diagnostics. The Rust
binding uses the pinned account-device MDK provider and the genuine SQLCipher
provider. The package contains no upstream application source. The lock pins
all registry and Git library dependencies.

Generate with Oddlang `842fcc87d94dbe77b689e80b4801628dd7b8fd20` using `oddlang build native-package rust
oddlang/portable/native-encrypted-init-package.json OUTPUT`, compare
`OUTPUT/package.lock` with `oddlang/portable/native-encrypted-init-v1/package.lock`,
and build `OUTPUT` using Cargo 1.97.1 release, locked and offline. The dependency
cache must include the third-party Git repositories identified in Cargo.lock.
The independent inspection program uses rusqlite 0.40.1 with
`bundled-sqlcipher-vendored-openssl`, the same pinned SQLCipher replacement,
serde_json 1.0.151 and zeroize 1.9.1. It identifies wrong key rejection without
printing keys and counts relationships without writing to either database.

Run `python3 -B encrypted_init_probe.py ORIGINAL CANDIDATE INSPECTOR RECEIPT
--framework-revision REVISION --generated-owner GENERATED_MARMISHD`. The probe
compares both profile trees and reads both encrypted databases before starting
an original consumer that could initialize missing state. It then runs the
generated owner and client against the generated profile and checks init
cross-consumption in both directions.

The macOS and Linux ARM64 receipts record the same finite corrupted-journal
failure. Linux also emits the matching SQLCipher provider trace before the
named error. The probe strips only timestamps from those provider lines and
records the message content.
