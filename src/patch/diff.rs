use anyhow::{bail, Context, Result};

use crate::{
    line::{write_lines_to, Line},
    patch::hunk::Hunk,
    utils::split_before,
};

/// The parts of a diff that represent line based differences in a
/// file (as opposed to pure renames (or deletions?)).
pub struct DiffDifferences<'a> {
    pub index_line: Option<Line<'a>>,
    pub minus_line: Line<'a>,
    pub plus_line: Line<'a>,
    pub hunks: Vec<Hunk<'a>>,
}

/// A bare diff for a single file. (A Patch file represents any number
/// of Diff instances.)
pub struct Diff<'a> {
    // The line that starts with "diff "
    pub diff_line: Line<'a>,
    pub newfile_line: Option<Line<'a>>,
    pub deleted_line: Option<Line<'a>>,
    pub similarity_line: Option<Line<'a>>,
    pub rename_from_line: Option<Line<'a>>,
    pub rename_to_line: Option<Line<'a>>,
    // unused
    pub differences: Option<DiffDifferences<'a>>,
}

impl<'a> Diff<'a> {
    /// Not the head of the patch (i.e. mail headers / commit
    /// message), but of this diff. Ends with a newline.
    pub fn head(&self) -> Vec<u8> {
        let Self {
            diff_line,
            newfile_line,
            deleted_line,
            similarity_line,
            rename_from_line,
            rename_to_line,
            differences,
        } = self;
        let mut head: Vec<u8> = Vec::new();

        let lines = [
            Some(diff_line),
            newfile_line.as_ref(),
            deleted_line.as_ref(),
            similarity_line.as_ref(),
            rename_from_line.as_ref(),
            rename_to_line.as_ref(),
        ]
        .into_iter()
        .flatten();

        write_lines_to(lines, &mut head).expect("writing to Vec doesn't fail");
        if let Some(differences) = differences {
            let DiffDifferences {
                index_line: _,
                minus_line,
                plus_line,
                hunks: _,
            } = differences;

            let lines = [Some(minus_line), Some(plus_line)].into_iter().flatten();
            write_lines_to(lines, &mut head).expect("writing to Vec doesn't fail");
        }

        head
    }

    pub fn from_lines(mut lines: impl Iterator<Item = Line<'a>>) -> Result<Diff<'a>> {
        let diff_line = lines
            .next()
            .filter(|l| l.starts_with(b"diff "))
            .with_context(|| format!("missing `diff ` line"))?;

        let line = lines
            .next()
            .with_context(|| format!("unexpected end of diff after line {diff_line}"))?;

        let (newfile_line, line) = if line.starts_with(b"new file mode ") {
            (
                Some(line),
                lines
                    .next()
                    .with_context(|| format!("unexpected end of diff after line {line}"))?,
            )
        } else {
            (None, line)
        };

        let (deleted_line, line) = if line.starts_with(b"deleted ") {
            (
                Some(line),
                lines
                    .next()
                    .with_context(|| format!("unexpected end of diff after line {line}"))?,
            )
        } else {
            (None, line)
        };

        let (similarity_line, line) = if line.starts_with(b"similarity ") {
            (
                Some(line),
                lines
                    .next()
                    .with_context(|| format!("unexpected end of diff after line {line}"))?,
            )
        } else {
            (None, line)
        };

        let (rename_from_line, line) = if line.starts_with(b"rename from ") {
            (
                Some(line),
                lines
                    .next()
                    .with_context(|| format!("unexpected end of diff after line {line}"))?,
            )
        } else {
            (None, line)
        };

        let (rename_to_line, line) = if line.starts_with(b"rename to ") {
            (Some(line), lines.next())
        } else {
            (None, Some(line))
        };

        let differences = if let Some(line) = line {
            let (index_line, line) = if line.starts_with(b"index ") {
                (
                    Some(line),
                    lines
                        .next()
                        .with_context(|| "unexpected end of diff after line {line}")?,
                )
            } else {
                (None, line)
            };

            if !line.starts_with(b"--- ") {
                bail!("expected `--- ` on line {line}");
            }
            let minus_line = line;

            let line = lines
                .next()
                .with_context(|| "unexpected end of diff after line {line}")?;
            if !line.starts_with(b"+++ ") {
                bail!("invalid patch file format: expected `+++ ` on line {line}");
            }
            let plus_line = line;

            let hunks = gather_hunks(lines);

            Some(DiffDifferences {
                index_line,
                minus_line,
                plus_line,
                hunks,
            })
        } else {
            None
        };

        Ok(Diff {
            diff_line,
            newfile_line,
            deleted_line,
            similarity_line,
            rename_from_line,
            rename_to_line,
            differences,
        })
    }
}

fn gather_hunks<'s>(lines: impl Iterator<Item = Line<'s>>) -> Vec<Hunk<'s>> {
    split_before(
        lines,
        |line| line.starts_with(b"@@ "),
        |group| Hunk { lines: group },
    )
}
