# Native status endpoint discovery

The [standalone package](../native-status-discovery-package.json) binds the typed status client to the original profile and runtime layout. The native client accepts an explicit state directory, or selects Linux's absolute `XDG_DATA_HOME` and the platform's fallback `HOME` suffix. It uses the explicit socket when supplied after state selection. Otherwise it selects the CLI identity, with `default` supplied by the CLI contract.

The provider checks private directories and reads the selected `profile.json` and the temporary-directory `binding.json` under the native UID and profile ID. Ordered rules validate the manifest, bind the runtime record back to the canonical profile path, and check the socket path's native byte width. The original status entry path performs these checks without loading command policy from `config.json`. The provider performs the same one-request exchange with the resulting path.

[Source provenance](provenance.json) · [Endpoint obligations](endpoint-obligations.json) · [Native qualification](../../../verification/whole/status/discovery-README.md)
