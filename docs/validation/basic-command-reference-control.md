# Basic-command reference control on macOS ARM64

The independent fixture replay passed all five short cases and four finite vectors. It checked exact process exit status, both binary streams and every field in the fixture's independent server-side receipt. The pinned native fixture was rebuilt from `af54339d250290490d62d4475a77539b7e739071` with Cargo 1.97.1, `--offline`, `--locked` and the `installed-checks` feature. The original release set came from the existing macOS ARM64 bridge build. The loopback plaintext and private-certificate rejection replay matched its tracked receipt byte for byte.

The two independently selected mixed pairings both reached generated `setup`. The generated executable returned 125 with empty stdout and the exact stderr bytes for `command operation unavailable` followed by LF. The upstream client setup succeeded in the upstream-client direction. Neither mixed pairing attempted a command. The candidate export lacked bridge, engine worker, worker, credential helper and release-tool executables. The observer used the supplied candidate with no upstream executable on its generated side.

The observer checked write confinement, denied access to the real user home and default Keychain and created disposable file profiles only after those probes. It removed both mixed workspaces. The final recorded attempts include their host metadata comparison and cleanup status. An earlier generated-client attempt recorded a host metadata change during the run and remains outside the clean measurements. No specific host writer was identified.

In the recorded upstream control, both original setup invocations succeeded and the configured authorization was verified. The control then stopped at owner activation before attempting a command. Cleanup and the host metadata check passed. The complete upstream command oracle remains open. See the receipt and replay commands under `verification/whole/basic-command-interop/`.

## Serial owner replay

A later run with the sandbox directory-stat repair and private named daemon logs used the same pinned peers, fresh distinct identities and fresh owned relay. Both owners reported active. The first foreground case expected exit 0 and `done\n`. It returned exit 255 with empty stdout and retained only a digest of its dynamic stderr. The receiver receipt remained unverified, so the corpus stopped before detached execution or warm repetitions. Cleanup passed. The tracked serial replay receipt binds the observed bytes and elapsed time. The cause of the native owner’s command completion failure remains open.

## Sealed setup candidate

The companion candidate built from `d187bf9047c2755ac5a6969fc66bee611a966eee` passed the mixed setup stage in both directions on macOS ARM64. Both peers returned setup exit 0. Distinct file-backed credentials, exact peer authorization, the sole owned relay and cleanup were verified for each attempt. The upstream-client pairing stopped at generated server foreground owner exit 125. In the generated-client pairing the upstream server reached active before the generated client owner exited 125. Each pairing used an independent fresh relay and profile workspace. The command corpus was not run in these mixed attempts.

The exact artifact manifest and independently run setup preflight are recorded as `profile-setup-initial-build-macos-arm64.json` and `profile-setup-initial-preflight-macos-arm64.json` in the interoperability verification directory. Their digests match the pins in both mixed receipts.

## Full upstream corpus with failed host metadata check

The readiness-gated upstream replay matched all 24 case outcomes and receiver receipts, including one cold and three warm finite payloads. The observer removed its workspace. Its host metadata comparison failed, so the receipt records a blocked run and leaves the reference incomplete. A later metadata-only check found mtimes inside the test interval for the Keychain directory and one immediate entry. The original per-path snapshots were discarded at teardown, so the exact changed tuple and the writer remain unknown. The observer now records only changed metadata categories to make later failures attributable to a monitored scope without publishing paths or credential contents.
