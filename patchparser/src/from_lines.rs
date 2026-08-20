use bumpalo::Bump;

use crate::line::Line;

/// This trait is somewhat obsolete, because often more information is
/// needed (e.g. for how to check parsing inconsistencies), thus other
/// `from_lines` methods are implemented outside the trait now; also
/// it was originally for converting stringified representations
/// automatically back, but that has been removed.
pub trait FromLines<'t>: Sized {
    fn from_lines(lines: &'t [Line<'t>], bump: &'t Bump) -> Result<Self, anyhow::Error>;
}
