pub mod change;
pub mod change_line;
pub mod diff;
pub mod hunk;
pub mod parsed_hunk;
#[allow(clippy::module_inception)]
// XXX rename or re-export
pub mod patch;
