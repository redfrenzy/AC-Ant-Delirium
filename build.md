# Geode Build and Governance Trigger

/// [WHAT] Build-time orchestration and governance activation.
/// [WHY] To bridge the local geode with the project-wide governance auditor.
/// [WHERE] Project root build trigger (Cargo integration).

This script ensures that every compilation of `ac_ant_delirium` involves a full governance audit.
It triggers the legacy `ant_delirium` engine to verify compliance with Elite standards.
