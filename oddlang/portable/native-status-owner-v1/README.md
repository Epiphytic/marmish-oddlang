# Native read-only status owner

The manually recovered graph defines the private Unix socket protocol for the original daemon's activation status. The generated executable takes an explicit socket path. It binds the endpoint, publishes the actual locally bound phase as `activating`, accepts same-user requests, and closes admission and removes its owned socket on shutdown.

The original daemon can later publish `active` and `transport_degraded` around its provider work. This graph declares no such transitions. It has no relay or execution provider and remains `activating` until the local listener stops. The standalone executable is `marmish-status-owner`. It has a separate package from the experimental client and does not replace the installed `marmishd` role.

The closed status request is `{"operation":"status"}` in u32 big-endian length framing. The server has the original frame, path, concurrency and I/O limits. It requires an owner-only directory, binds socket mode 0600 and refuses all occupied paths, including stale sockets. The original can remove a stale socket under its profile lease. This package has no profile lease and leaves stale paths untouched.

The [reverse qualification](../../../verification/whole/status-owner/README.md) compares the original native client against the generated owner and the independent original daemon, including framed errors and SIGTERM exit and socket cleanup. The original's provider shutdown report remains outside the local owner profile.
