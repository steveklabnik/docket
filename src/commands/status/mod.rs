//! Status management commands for bugs.
//!
//! This module handles bug status transitions:
//! - `approve` - Mark a bug as approved for work
//! - `done` - Mark a bug as complete (with optional jj integration)

mod done;
mod jj;
mod transitions;

pub use done::done;
pub use transitions::approve;
