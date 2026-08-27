//! Internal Pactrun architecture facade.
//!
//! This crate does not expose a stable library API. Modules remain crate-private
//! until real contracts justify a narrower interface.

mod application;
mod authoring;
mod cli;
mod domain;
mod executor;
mod hook;
mod managed_data;
mod persistence;
mod workflow;
