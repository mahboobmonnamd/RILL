//! Orchestration plane (AGENTS.md §5): config resolution, trust/redaction,
//! the agent `Task` object, and the attention queue. Cold. No PTY, no
//! display, no warm-path frames — nothing here may be called from the
//! typing path.

pub mod attention;
pub mod config;
pub mod task;
pub mod trust;
