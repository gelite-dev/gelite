//! Shared command orchestration for Gelite tools.
//!
//! This crate belongs to the tools layer. It composes parser, planner,
//! renderer, and runner crates into user-facing commands, but it does not own
//! process argument parsing, stdout/stderr, or process exit codes.
