# Generated foreground relay review

Claude Code reviewed the framework and companion activation inputs in auto mode. The first pass found that a lost relay left the generated owner reporting readiness and that a relay directory could coincide with an excluded profile entry. The framework now monitors the live connection and notification forwarder while serving status and rejects the conflicting graph before emission. The acceptance fixture stops the test relay after a second startup and requires the daemon to withdraw readiness, report transport failure and clean up its runtime.

The follow-up adds explicit authentication before publication and requires acknowledgement from the declared relay. The test relay's pinned event policy checks the event signature and authenticated author before storing a KeyPackage. The fixture independently reads the resulting event from the disposable relay store and compares it after a restart.

The follow-up review confirmed that the generated JSON renames the provider report field to `mdk_stop_complete`. The fixture now checks the complete report and records the exact observable outcome on relay loss. The daemon exits with the transport failure and removes the socket. The receipt does not claim that an intermediate status transition was observed.

The final Claude Code auto-mode verification review found no remaining blocker. The exact report controls passed again against the source-denied release artifacts. The [live receipt](../../verification/whole/basic-command-interop/generated-provider-relay-aarch64-apple-darwin.json) binds the fixture and binary digests to the observed outcome.
