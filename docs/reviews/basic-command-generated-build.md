# Basic-command generated build review

Claude Opus 5.5 reviewed commit `ecd38f7` in Claude Code auto mode. The reviewed scope covered the portable input closure, isolated build, dependency verification, artifact export, and candidate-only preflight.

The first review found that the build receipt omitted the helpers responsible for staging and Seatbelt policy. The revision pins those helpers and the effective policy, records the build driver checkout, and requires clean portable inputs and verifier directories. It also adds an independent default-deny canary, inventories all generated non-target files, binds CLI surfaces to declared binaries, records tool digests, and publishes the completed export by rename.

The follow-up approved the limited build and preflight evidence with no blocking findings. The reviewer noted that the receipt records the effective policy digest and denial-probe evidence. Host allowlist paths remain in the builder and its helpers. File syscall tracing, live interoperability, and exercised peer-side role attribution remain open. The review was static. Runtime evidence is recorded separately in [the build instructions](../../verification/whole/basic-command-interop/BUILD.md).
