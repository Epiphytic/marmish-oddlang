# Native foreground provider lifetime review

Claude approved compiler revision `e2fa69c3f18c0ac66f2ef4a0cad2c18e5e7dc98c` and companion revision `f7535d7f4d45270779d1bf0a5690b0c4f7d05554`. Framework review identified two incomplete barriers. The retained account database now has an explicit native release check, and the relay emits its terminal notification before the forwarder is joined, including its delivery worker. Executor destruction still precedes lease release when a stage fails or times out.

Companion review asked to verify the actual generated Cargo manifest and strengthen the held TLS observation. The sealed build checks the manifest alongside the lock and generated source inventory. The native observer checks that the original and generated daemons each still own the TLS connection immediately before receiving the signal. It observes EOF and checks for an additional accepted connection. Independent file-descriptor and encrypted readback witnesses establish storage ownership during activation and usable profile state after shutdown.

The qualification covers IPv4 loopback with prepared current-format private file profiles. It includes the original and generated initializers, owners and discovering clients. WebSocket transport and provider readiness, replay-floor advancement, nonempty or older stores, configured execution and installed services remain open.

[Framework PR](https://github.com/Epiphytic/oddlang/pull/65) · [Native qualification](../../verification/whole/foreground-lifecycle/README.md)
