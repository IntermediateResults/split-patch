use anyhow::{bail, Context, Result};
use itertools::Itertools;

use crate::{line::Line, patch::hunk::Hunk, utils::split_before};

/// A bare diff for a single file. (A Patch file represents any number
/// of Diff instances.)
pub struct Diff<'a> {
    // The line that starts with "diff "
    pub diff_line: Line<'a>,
    pub newfile_line: Option<Line<'a>>,
    // unused
    pub index_line: Option<Line<'a>>,
    pub minus_line: Line<'a>,
    pub plus_line: Line<'a>,
    pub hunks: Vec<Hunk<'a>>,
}

impl<'a> Diff<'a> {
    /// Not the head of the patch (i.e. mail headers / commit
    /// message), but of this diff. Ends with a newline.
    pub fn head(&self) -> String {
        let Self {
            diff_line,
            newfile_line,
            index_line: _,
            minus_line,
            plus_line,
            hunks: _,
        } = self;
        [
            Some(*diff_line),
            *newfile_line,
            Some(*minus_line),
            Some(*plus_line),
        ]
        .into_iter()
        .flatten()
        .map(|line| *line)
        .join("\n")
            + "\n"
    }

    pub fn from_lines(mut lines: impl Iterator<Item = Line<'a>>) -> Result<Diff<'a>> {
        let diff_line = lines
            .next()
            .filter(|l| l.starts_with("diff "))
            .with_context(|| format!("invalid patch file format: missing 'diff ' line"))?;

        let line = lines.next().with_context(|| {
            format!(
                "invalid patch file format: unexpected EOF after 'diff ' line on line {diff_line}",
            )
        })?;

        let (newfile_line, line) = if line.starts_with("new file mode ") {
            (
                Some(line),
                lines.next().with_context(|| {
                    format!(
                        "invalid patch file format: [missing `index` or `---` line] after line {line}",
                    )
                })?,
            )
        } else {
            (None, line)
        };

        let (index_line, line) = if line.starts_with("index ") {
            (
                Some(line),
                lines.next().with_context(|| {
                    format!("invalid patch file format [missing `---` line] after line {line}",)
                })?,
            )
        } else {
            (None, line)
        };

        if !line.starts_with("--- ") {
            bail!("invalid patch file format: expected `---` on line {line}",);
        }
        let minus_line = line;

        let line = lines.next().with_context(|| {
            format!("invalid patch file format [missing `+++` line] on line {line}",)
        })?;
        if !line.starts_with("+++ ") {
            bail!("invalid patch file format: expected `+++` on line {line}",);
        }
        let plus_line = line;

        let hunks = gather_hunks(lines);

        Ok(Diff {
            diff_line,
            newfile_line,
            index_line,
            minus_line,
            plus_line,
            hunks,
        })
    }
}

fn gather_hunks<'s>(lines: impl Iterator<Item = Line<'s>>) -> Vec<Hunk<'s>> {
    split_before(
        lines,
        |line| line.starts_with("@@ "),
        |group| Hunk { lines: group },
    )
}
