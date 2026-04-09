# Safety and Token Governance

/// [WHAT] Scanning for prohibited and restricted code tokens.
/// [WHY] To prevent unhandled panic paths and insecure coding patterns.
/// [WHERE] Safety audit phase of the rule engine.

This module monitors the use of "Poison Tokens" such as `unwrap` and `expect`.
It enforces explicit safety justifications via the `[Safety Required]` protocol.
