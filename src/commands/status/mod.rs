//! Status management commands for bugs.
//!
//! This module handles bug status transitions:
//! - `approve` - Mark a bug as approved for work
//! - `review` - Submit a bug for code review
//! - `reject` - Reject a bug from review back to in progress
//! - `done` - Mark a bug as complete (with optional jj integration)
//! - `block` - Mark a bug as blocked on external dependency
//! - `unblock` - Mark a blocked bug as back in progress
//! - `pause` - Intentionally set aside a bug
//! - `resume` - Resume work on a paused bug

mod block;
mod done;
mod jj;
mod pause;
mod reject;
mod resume;
mod review;
mod transitions;
mod unblock;

pub use block::block;
pub use done::done;
pub use pause::pause;
pub use reject::reject;
pub use resume::resume;
pub use review::review;
pub use transitions::approve;
pub use unblock::unblock;
