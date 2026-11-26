# Pending file profile credential preparation

The standalone `marmish-profile-prepare prepare` operation creates private
credentials and leaves a new file-backed profile in the initializing phase. An
independent macOS ARM64 witness checks ownership, modes, random references,
x-only public identity, lease exclusion and preservation on retry. The witness
then gives the pending profile to pinned upstream and separately reads an
upstream-created ready profile with the generated tool.

Build with Oddlang revision `6e986370a7f0a84efdae709c0e2bb3c193f62c11` and
`oddlang build native-package rust oddlang/portable/native-file-profile-package.json OUTPUT`.
Compare the generated package lock with
`oddlang/portable/native-file-profile-v1/package.lock`, then run
`cargo +1.97.1 build --release --locked --offline --manifest-path OUTPUT/Cargo.toml`.
Run `file_profile_probe.py ORIGINAL CANDIDATE RECEIPT --framework-revision REVISION`.
Its receipt records independent initial credentials and output identity; it never
records generated key bytes. Upstream creates the stores after the witness first
asserts their absence. Ordinary init, encrypted store creation and counts remain
separate gates.
