# Core package review

Claude Code with Claude Opus 5.5 independently reviewed the core package
increment in auto permission mode. The review covered the package inputs
and lock, the Rust 2021 release build, the unchanged upstream tests, native
differential and mutation witnesses, and both platform receipts.

The review reproduced macOS qualification and the byte-identical receipt.
It independently checked the generated package and public API witness against
their pinned inputs. It also checked the Linux runner and compared the Linux
receipt against the committed inputs and macOS transcript. The Linux replay
was run separately in the native ARM64 guest.

The first review found that the macOS denylist left a second upstream
checkout readable. The revised runner uses a read allowlist and verifies
that a byte-identical copy of upstream source outside that allowlist is
unreadable. The Linux follow-up binds the framework checkout, rehashes all
generated files and records the actual privilege and network isolation
checks. A focused auto-mode re-review approved those changes at `51b0f5c`.

The final review suggested checking the generated package root as well as
its source inventory. Both runners now verify the exact root entries. A
control rejects an additional build script, and both native runs reproduced
the original transcript after that check. The approved runtime receipt scope
is the Marmish core library at the pinned release reference.
