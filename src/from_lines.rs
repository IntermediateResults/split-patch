use bumpalo::Bump;

use crate::line::Line;

pub trait FromLines<'t>: Sized {
    fn from_lines(lines: &'t [Line<'t>], bump: &'t Bump) -> Result<Self, anyhow::Error>;
}
