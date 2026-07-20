use std::{fmt::Display, io::Write, ops::Deref};

/// A reference to a line string without the line ending and the line
/// number for location reporting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line<'a> {
    /// 0-based line number
    line_no0: usize,
    s: &'a str,
}

impl<'a> Deref for Line<'a> {
    type Target = &'a str;

    fn deref(&self) -> &Self::Target {
        &self.s
    }
}

impl<'a> Display for Line<'a> {
    /// Display line number rather than the line contents
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.line_no())
    }
}

impl<'a> Line<'a> {
    pub fn from_tuple((line_no0, s): (usize, &'a str)) -> Self {
        Self { line_no0, s }
    }

    /// line contents without newline
    pub fn s(&self) -> &'a str {
        self.s
    }

    /// 0-based line number
    pub fn line_no0(&self) -> usize {
        self.line_no0
    }

    /// 1-based line number
    pub fn line_no(&self) -> usize {
        self.line_no0 + 1
    }
}

/// Write the line strings out with line endings added
pub fn write_lines_to<'a>(
    lines: impl IntoIterator<Item = &'a Line<'a>>,
    mut out: impl Write,
) -> Result<(), std::io::Error> {
    for line in lines {
        out.write_all(line.as_bytes())?;
        out.write_all(b"\n")?;
    }
    Ok(())
}
