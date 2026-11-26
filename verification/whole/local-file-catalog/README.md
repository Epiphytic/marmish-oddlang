# Read-only directory catalog verification

The [separate candidate](../../../oddlang/portable/workspace-v7-local-file-catalog.json) links a private catalog validator to the existing typed line grammar and read-only file policy. It checks inert local data relative to a trusted fixture directory. The caller supplies the expected SHA-256. Versioned discovery counts matching UTF-8 names before reading either candidate, including malformed matching names. The fixed-name branch applies the declared ownership and low-mode checks. The test bodies only create data files.

The runner extracts `native_manifest::tests::versioned_directory_discovery_supports_both_families_and_rejects_ambiguity` and its sample helper from the pinned original. It verifies both hashes, builds the generated native library before attaching either, and requires the exact case to execute once. Twelve independent observations exercise both naming modes and distinguish ambiguous selection, parse/checksum precedence, size and digest failures, and fixed-mode rejection. Removing the 0.3 family makes the unchanged test fail at the decoy-ambiguity assertion, which the runner checks by staged source location.

Only the native member changes. The original tempfile provider is a native development dependency for the unchanged test. The profile supplies no release-tool or command wiring. This gate checks an exact individual test and the independent corpus on this candidate. It does not measure a fresh aggregate named-suite count.

Run with clean pinned checkouts and already populated offline Cargo caches.

```sh
python3 -B verification/whole/local-file-catalog/compose.py "$ORIGINAL" --check
python3 -B verification/whole/local-file-catalog/verify.py \
  "$ODDLANG" "$COMPILER_CACHE" "$ORIGINAL" "$ORIGINAL_TARGET" /tmp/local-file-catalog.json \
  --framework-revision 3543b8cb296c24f2b77a6e98ef4f502b92e4e589
```

The [macOS ARM64](aarch64-apple-darwin.json) and [Linux ARM64](aarch64-unknown-linux-gnu.json) runs observed the same named test, twelve outcomes and mutation result. Both used workspace lock `6cc00cdb304db004d4fc7ca7433155442dc7c34170123441904ab696a9d9551c`.
