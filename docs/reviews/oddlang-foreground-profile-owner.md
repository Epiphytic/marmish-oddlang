# Native foreground profile owner review

Claude Opus 5.5 approved framework revision `543ebcd3cb47de0f74a4da70990b1c3d584e7c63`, companion generation revision `2f99d8125578ceb9ce279cab08f78431d17387c0`, and the receipts published at `1d68f118545e03d0c0282dbd702419ca3bfb1380`.

Framework review identified crash recovery and backend selection gaps. The owner now verifies a compatible dead runtime under its lease, preserves conflicting entries and live endpoints, and maps each supported CLI backend to the persisted file provider. The unrelated release fixture checks these paths and actual TLS activation.

Companion review identified a shared test listener that could misattribute a ClientHello. Each measured daemon and competitor now uses its own relay listener. The independent native ARM64 receipts agree on held TLS, exact original-client status and cross-implementation lease results, trust rejection, signal cleanup, reattachment, kill recovery, and unchanged parser observations. The activation profile ends at TLS and local ownership. WebSocket and MDK readiness remain open.

[Companion PR](https://github.com/Epiphytic/marmish-oddlang/pull/35) · [Native qualification](../../verification/whole/foreground-owner/README.md)
