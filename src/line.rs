use std::{fmt::Display, io::Write, ops::Deref};

use crate::bumpalo_cow::CloneIn;

/// A reference to a line string without the line ending and the line
/// number for location reporting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line<'a> {
    /// 0-based line number; MAX represents "generated" (no location)
    line_no0: usize,
    contents: &'a [u8],
}

impl<'a> Deref for Line<'a> {
    type Target = &'a [u8];

    fn deref(&self) -> &Self::Target {
        &self.contents
    }
}

// XX painful, really no way out?
impl<'a> CloneIn<'a> for Line<'a> {
    fn clone_in(&self, _bump: &'a bumpalo::Bump) -> Self {
        *self
    }
}

impl<'a> Display for Line<'a> {
    /// Display line number rather than the line contents
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(line_no) = self.line_no() {
            write!(f, "{line_no}")
        } else {
            f.write_str("(no location)")
        }
    }
}

impl<'a> Line<'a> {
    pub fn from_tuple((line_no0, contents): (usize, &'a [u8])) -> Self {
        Self { line_no0, contents }
    }

    pub fn from_generated_content(contents: &'a [u8]) -> Self {
        Self {
            line_no0: usize::MAX,
            contents,
        }
    }

    /// line contents without newline
    pub fn contents(&self) -> &'a [u8] {
        self.contents
    }

    pub fn set_contents(&mut self, contents: &'a [u8]) {
        self.contents = contents;
    }

    /// 0-based line number; None if the line was generated (has no location)
    pub fn line_no0(&self) -> Option<usize> {
        if self.line_no0 == usize::MAX {
            None
        } else {
            Some(self.line_no0)
        }
    }

    /// 1-based line number; None if the line was generated (has no location)
    pub fn line_no(&self) -> Option<usize> {
        self.line_no0().map(|no| no + 1)
    }
}

/// Write the line strings out with line endings added
pub fn write_lines_to<'a>(
    lines: impl IntoIterator<Item = &'a Line<'a>>,
    mut out: impl Write,
) -> Result<(), std::io::Error> {
    for line in lines {
        out.write_all(line)?;
        out.write_all(b"\n")?;
    }
    Ok(())
}
