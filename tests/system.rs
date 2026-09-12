//! Executable-level system scenarios for the public Pactrun CLI boundary.
//!
//! Each scenario owns an isolated storage root and starts a fresh `pactrun`
//! process for every product operation and inspection.

#[path = "system/common.rs"]
mod common;
#[cfg(target_os = "linux")]
#[path = "system/linux.rs"]
mod linux;
#[path = "system/matrix.rs"]
mod matrix;
#[path = "system/snapshots.rs"]
mod snapshots;
#[path = "system/support/mod.rs"]
mod support;
#[cfg(windows)]
#[path = "system/windows.rs"]
mod windows;
