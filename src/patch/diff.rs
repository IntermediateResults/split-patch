use anyhow::{bail, Context, Result};
use itertools::Itertools;

use crate::{patch::hunk::Hunk, utils::split_before};

/// A bare diff for a single file. (A Patch file represents any number
/// of Diff instances.)
pub struct Diff<'a> {
    pub diff_line: &'a str,
    pub newfile_line: Option<&'a str>,
    // unused
    pub index_line: Option<&'a str>,
    pub minus_line: &'a str,
    pub plus_line: &'a str,
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
        .join("\n")
            + "\n"
    }

    // Can't impl FromStr since we want to carry over the argument
    // life time
    pub fn from_str(diff: &'a str) -> Result<Diff<'a>> {
        let mut lines = diff.lines().enumerate();

        let (_, diff_line) = lines
            .next()
            .filter(|(_, l)| l.starts_with("diff "))
            .with_context(|| format!("invalid patch file format: missing 'diff ' line"))?;

        let (line0, line) = lines.next().with_context(|| {
            format!("invalid patch file format: unexpected EOF after 'diff ' line")
        })?;

        let (newfile_line, (line0, line)) = if line.starts_with("new file mode ") {
            (
                Some(line),
                lines.next().with_context(|| {
                    format!("invalid patch file format: [missing `index` or `---` line]")
                })?,
            )
        } else {
            (None, (line0, line))
        };

        let (index_line, (line0, line)) = if line.starts_with("index ") {
            (
                Some(line),
                lines.next().with_context(|| {
                    format!("invalid patch file format [missing `---` line]: {diff}")
                })?,
            )
        } else {
            (None, (line0, line))
        };

        if !line.starts_with("--- ") {
            bail!(
                "invalid patch file format: expected `---` on line {}",
                line0 + 1
            );
        }
        let minus_line = line;

        let (line0, line) = lines
            .next()
            .with_context(|| format!("invalid patch file format [missing `+++` line]: {diff}"))?;
        if !line.starts_with("+++ ") {
            bail!(
                "invalid patch file format: expected `+++` on line {}",
                line0 + 1
            );
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

fn gather_hunks<'s>(lines: impl Iterator<Item = (usize, &'s str)>) -> Vec<Hunk<'s>> {
    split_before(
        lines,
        |(_, line)| line.starts_with("@@ "),
        |group| Hunk { lines: group },
    )
}
