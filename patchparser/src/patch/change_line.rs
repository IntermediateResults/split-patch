use anyhow::{anyhow, Result};

use crate::line::Line;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeLineKind {
    /// ' '
    Context,
    /// '+'
    Plus,
    /// '-'
    Minus,
    /// "\ No newline at end of file"
    Backslash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeTerminator {
    // "diff "
    Diff,
    // "@@ "
    Hunk,
}

#[derive(Debug)]
pub(crate) enum ChangeLineReport {
    Kind(ChangeLineKind),
    Terminator(ChangeTerminator),
    InvalidSyntax(anyhow::Error),
}

impl From<Line<'_>> for ChangeLineReport {
    fn from(line: Line) -> Self {
        use ChangeLineKind::*;
        use ChangeLineReport::*;
        use ChangeTerminator::*;
        match line.first() {
            Some(c) => match *c {
                b' ' => Kind(Context),
                b'+' => Kind(Plus),
                b'-' => Kind(Minus),
                b'\\' => Kind(Backslash),
                _ => {
                    if line.starts_with(b"@@ ") {
                        Terminator(Hunk)
                    } else if line.starts_with(b"diff ") {
                        Terminator(Diff)
                    } else {
                        InvalidSyntax(anyhow!(
                            "invalid syntax in change on line {line}: unknown line start"
                        ))
                    }
                }
            },
            None => InvalidSyntax(anyhow!(
                "invalid syntax in change on line {line}: empty line"
            )),
        }
    }
}

impl ChangeLineReport {
    /// Returns the type expected by `try_take_while` for predicates
    pub(crate) fn matches_kinds(self, kinds: &[ChangeLineKind]) -> Result<(), ChangeLineReport> {
        match self {
            ChangeLineReport::Kind(change_line_kind) => {
                if kinds.contains(&change_line_kind) {
                    Ok(())
                } else {
                    Err(self)
                }
            }
            _ => Err(self),
        }
    }

    pub(crate) fn kind_or_terminator(self) -> Result<ChangeLineReport> {
        match self {
            ChangeLineReport::InvalidSyntax(error) => Err(error),
            t => Ok(t),
        }
    }
}

pub trait SeparateErrors: Sized {
    type Error;
    fn separate_errors(self) -> Result<Self, Self::Error>;
}

impl SeparateErrors for Option<ChangeLineReport> {
    type Error = anyhow::Error;

    fn separate_errors(self) -> Result<Self, Self::Error> {
        match self {
            Some(report) => Ok(Some(report.kind_or_terminator()?)),
            None => Ok(None),
        }
    }
}
