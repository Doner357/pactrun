//! Drive crate-private production codec conformance without a public Rust API.

use std::path::Path;

pub(crate) fn verify(workspace_root: &Path) -> Result<(), String> {
    super::revision_core_v1::verify_traceability(workspace_root)?;
    // The selected tests verify production V1 against unchanged Frozen vectors,
    // production V2 against its own corpus, and the independent Node 24 oracle.
    super::run(
        workspace_root,
        "cargo",
        &[
            "test",
            "-p",
            "pactrun",
            "--lib",
            "--all-features",
            "snapshot_integrity::tests",
        ],
    )
}
