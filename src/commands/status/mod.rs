//! Status management commands for bugs.
//!
//! This module handles bug status transitions:
//! - `approve` - Mark a bug as approved for work
//! - `review` - Submit a bug for code review
//! - `reject` - Reject a bug from review back to in progress
//! - `done` - Mark a bug as complete (with optional jj integration)

mod done;
mod jj;
mod reject;
mod review;
mod transitions;

pub use done::done;
pub use reject::reject;
pub use review::review;
pub use transitions::approve;
