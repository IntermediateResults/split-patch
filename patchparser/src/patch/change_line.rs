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

impl ChangeLineKind {
    pub fn prefix(self) -> u8 {
        use ChangeLineKind::*;
        match self {
            Context => b' ',
            Plus => b'+',
            Minus => b'-',
            Backslash => b'\\',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeTerminator {
    // "diff "
    Diff,
    // "@@ "
    Hunk,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum KindOrTerminator {
    Kind(ChangeLineKind),
    Terminator(ChangeTerminator),
}

impl KindOrTerminator {
    pub(crate) fn kind(&self) -> Option<ChangeLineKind> {
        match self {
            KindOrTerminator::Kind(change_line_kind) => Some(*change_line_kind),
            KindOrTerminator::Terminator(_change_terminator) => None,
        }
    }
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
    #[allow(unused)]
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

    pub(crate) fn kind_or_terminator(self) -> Result<KindOrTerminator> {
        match self {
            ChangeLineReport::InvalidSyntax(e) => Err(e),
            ChangeLineReport::Kind(k) => Ok(KindOrTerminator::Kind(k)),
            ChangeLineReport::Terminator(t) => Ok(KindOrTerminator::Terminator(t)),
        }
    }
}
