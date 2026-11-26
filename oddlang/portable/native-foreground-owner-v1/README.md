# Foreground native profile owner

This package generates the ordinary `marmishd` executable for an existing ready file-backed profile. The qualified invocation is:

```sh
marmishd --foreground --state-dir STATE --identity PROFILE --key-storage file --tor off --relay wss://127.0.0.1:PORT
marmish --state-dir STATE --identity PROFILE status
```

The client command above is the pinned original native client. It finds the generated daemon through the ordinary profile and runtime binding, without an explicit socket argument. The generated daemon supports the implicit daemon operation and the explicit `daemon` subcommand in this lane.

Startup validates and locks the existing profile, checks the signing credential against its declared x-only public key, validates the private storage credential, and prepares the private discoverable runtime. It leaves the prepared profile in place. A real direct TCP and Rustls WebPKI TLS attempt follows. The daemon reports `activating` during a held TLS handshake and rejects an untrusted certificate. It exits and removes its local runtime after signal or activation failure. A successful TLS step would report the remaining provider unavailable, with no application readiness claim.

The independently generated endpoint-discovery graph and this writer's layout are compared during recovery. This package retains the earlier parser and explicit client status components. Fresh provisioning, provider database ownership, WebSocket/MDK activation, active/degraded status, installed service lifetimes, provider shutdown reports and non-status operations are open. A dead socket in a private runtime left by an interrupted owner can be recovered under the lease when the existing binding matches. Corrupt or conflicting runtimes remain untouched.

[Reproduce the qualification](../../../verification/whole/foreground-owner/README.md).
