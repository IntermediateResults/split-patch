use std::io::Write;

use anyhow::{bail, Context, Result};
use bstr::{BStr, BString};
use bumpalo::{
    collections::{self as bc, CollectIn},
    Bump,
};

use crate::{
    bumpalo_cow::BumpaloCow,
    bumpalo_utils::split_before_in,
    from_lines::FromLines,
    line::{write_lines_to, Line},
    patch::hunk::{Hunk, WriteAsHunk},
    reborrow_in::ReborrowIn,
    write_to::WriteTo,
};

/// The parts of a diff that represent line based differences in a
/// file (as opposed to pure renames (or deletions?)).
#[derive(Clone, PartialEq, Eq)]
pub struct DiffDifferences<'a> {
    pub index_line: Option<Line<'a>>,
    pub minus_line: Line<'a>,
    pub plus_line: Line<'a>,
    pub hunks: bc::Vec<'a, Hunk<'a>>,
}

impl<'a, 'b> ReborrowIn<'b> for DiffDifferences<'a>
where
    'a: 'b,
{
    type Reborrowed = DiffDifferences<'b>;

    fn reborrow_in(&self, bump: &'b Bump) -> DiffDifferences<'b> {
        let Self {
            index_line,
            minus_line,
            plus_line,
            hunks,
        } = self;
        DiffDifferences {
            index_line: index_line.clone(),
            minus_line: minus_line.clone(),
            plus_line: plus_line.clone(),
            hunks: hunks.iter().map(|v| v.reborrow_in(bump)).collect_in(bump),
        }
    }
}

/// A bare diff for a single file. (A Patch file represents any number
/// of Diff instances.)
#[derive(Clone, PartialEq, Eq)]
pub struct Diff<'a> {
    // The line that starts with "diff "
    pub diff_line: Line<'a>,
    // The first path including the leading "a/" or similar
    pub diff_path_a_full: Option<&'a BStr>,
    // The second path including the leading "b/" or similar
    pub diff_path_b_full: Option<&'a BStr>,

    pub newfile_line: Option<Line<'a>>,
    pub deleted_line: Option<Line<'a>>,
    pub similarity_line: Option<Line<'a>>,
    pub rename_from_line: Option<Line<'a>>,
    pub rename_to_line: Option<Line<'a>>,
    pub differences: Option<DiffDifferences<'a>>,
}

fn strip_leading_path_segment(s: &BStr) -> Result<&BStr> {
    if s.first() == Some(&b'/') {
        bail!("path is absolute: {s:?}")
    }
    for i in 0..s.len() {
        if s[i] == b'/' {
            for i in i + 1..s.len() {
                if s[i] != b'/' {
                    return Ok(&s[i..]);
                }
            }
            bail!("missing path segments after initial segment in: {s:?}")
        }
    }
    bail!("could not find '/' in: {s:?}")
}

#[test]
fn t_strip_leading_path_segment() {
    fn b<'t>(s: &'t str) -> &'t BStr {
        s.as_ref()
    }
    let t = strip_leading_path_segment;
    assert_eq!(t(b("a/hey")).unwrap(), b("hey"));
    assert_eq!(t(b("abc///de/f")).unwrap(), b("de/f"));
    assert_eq!(
        t(b("a/")).err().unwrap().to_string(),
        "missing path segments after initial segment in: \"a/\""
    );
    assert_eq!(
        t(b("/a/hey")).err().unwrap().to_string(),
        "path is absolute: \"/a/hey\""
    );
}

impl<'a, 'b> ReborrowIn<'b> for Diff<'a>
where
    'a: 'b,
{
    type Reborrowed = Diff<'b>;

    fn reborrow_in(&self, bump: &'b Bump) -> Diff<'b> {
        let Self {
            diff_line,
            diff_path_a_full,
            diff_path_b_full,
            newfile_line,
            deleted_line,
            similarity_line,
            rename_from_line,
            rename_to_line,
            differences,
        } = self;
        Diff {
            diff_line: *diff_line,
            diff_path_a_full: diff_path_a_full.clone(),
            diff_path_b_full: diff_path_b_full.clone(),
            newfile_line: newfile_line.clone(),
            deleted_line: deleted_line.clone(),
            similarity_line: similarity_line.clone(),
            rename_from_line: rename_from_line.clone(),
            rename_to_line: rename_to_line.clone(),
            differences: differences.as_ref().map(|v| v.reborrow_in(bump)),
        }
    }
}

impl<'a> Diff<'a> {
    /// Replace the hunks with the given ones, while keeping file
    /// context information (except for deleting if
    /// `delete_index_line` is true).
    ///
    /// Panics if self does not contain a `DiffDifferences`.
    pub fn set_hunks(
        &mut self,
        hunks: bc::Vec<'a, Hunk<'a>>,
        delete_index_line: bool,
    ) -> &mut Self {
        let differences = self
            .differences
            .as_mut()
            .expect("`differences` required for setting hunks on Diff");
        differences.hunks = hunks;
        if delete_index_line {
            differences.index_line = None;
        }
        self
    }

    /// Not the head of the patch (i.e. mail headers / commit
    /// message), but of this diff. Ends with a newline.
    ///
    /// (Note: can't sensibly split `Diff` into a `DiffHead` that
    /// would implement `WriteTo`, since part of this "head" part is
    /// in `Diff.differences`.)
    pub fn write_head_to(
        &self,
        print_index_line: bool,
        mut out: impl Write,
    ) -> Result<(), std::io::Error> {
        let Self {
            diff_line,
            diff_path_a_full: _,
            diff_path_b_full: _,
            newfile_line,
            deleted_line,
            similarity_line,
            rename_from_line,
            rename_to_line,
            differences,
        } = self;

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

        write_lines_to(lines, &mut out)?;
        if let Some(differences) = differences {
            let DiffDifferences {
                index_line,
                minus_line,
                plus_line,
                hunks: _,
            } = differences;

            let lines = [
                if print_index_line {
                    index_line.as_ref()
                } else {
                    None
                },
                Some(minus_line),
                Some(plus_line),
            ]
            .into_iter()
            .flatten();
            write_lines_to(lines, &mut out)?;
        }

        Ok(())
    }

    /// Not the head of the patch (i.e. mail headers / commit
    /// message), but of this diff. Ends with a newline.
    pub fn head_to_bstring(&self, print_index_line: bool) -> BString {
        let mut head = BString::new(Vec::new());
        self.write_head_to(print_index_line, &mut *head)
            .expect("writing to Vec doesn't fail");
        head
    }

    pub fn write_hunks_to(&self, mut out: impl Write) -> Result<(), std::io::Error> {
        if let Some(differences) = &self.differences {
            for hunk in &differences.hunks {
                hunk.write_as_hunk_to(&mut out)?;
            }
        }
        Ok(())
    }

    pub fn diff_path_a(&self) -> Result<&BStr> {
        strip_leading_path_segment(
            self.diff_path_a_full
                .with_context(|| format!("missing first path in 'diff' line {}", self.diff_line))?,
        )
    }

    pub fn diff_path_b(&self) -> Result<&BStr> {
        strip_leading_path_segment(
            &self.diff_path_b_full.with_context(|| {
                format!("missing second path in 'diff' line {}", self.diff_line)
            })?,
        )
    }
}

fn gather_hunks<'s>(lines: &'s [Line<'s>], bump: &'s Bump) -> bc::Vec<'s, Hunk<'s>> {
    split_before_in(
        lines,
        |line| line.starts_with(b"@@ "),
        |group| Hunk {
            lines: BumpaloCow::Borrowed(group),
        },
        bump,
    )
}

impl<'a> WriteTo for Diff<'a> {
    fn write_to(&self, mut out: impl Write) -> Result<(), std::io::Error> {
        self.write_head_to(true, &mut out)?;
        self.write_hunks_to(&mut out)
    }
}

impl<'a> FromLines<'a> for Diff<'a> {
    fn from_lines(lines_slice: &'a [Line<'a>], bump: &'a Bump) -> Result<Diff<'a>> {
        let mut lines = lines_slice.into_iter();

        let diff_line = *lines
            .next()
            .filter(|l| l.starts_with(b"diff "))
            .with_context(|| format!("missing `diff ` line"))?;
        let (diff_path_a_full, diff_path_b_full);
        {
            let mut parts = diff_line.split(|b| *b == b' ');
            parts.next().expect("'diff' part was there");
            let mut parts = parts.skip_while(|p| p.starts_with(b"-"));
            diff_path_a_full = parts.next().map(AsRef::as_ref);
            diff_path_b_full = parts.next().map(AsRef::as_ref);
        }

        let line = *lines
            .next()
            .with_context(|| format!("unexpected end of diff after line {diff_line}"))?;

        let (newfile_line, line) = if line.starts_with(b"new file mode ") {
            (
                Some(line),
                *lines
                    .next()
                    .with_context(|| format!("unexpected end of diff after line {line}"))?,
            )
        } else {
            (None, line)
        };

        let (deleted_line, line) = if line.starts_with(b"deleted ") {
            (
                Some(line),
                *lines
                    .next()
                    .with_context(|| format!("unexpected end of diff after line {line}"))?,
            )
        } else {
            (None, line)
        };

        let (similarity_line, line) = if line.starts_with(b"similarity ") {
            (
                Some(line),
                *lines
                    .next()
                    .with_context(|| format!("unexpected end of diff after line {line}"))?,
            )
        } else {
            (None, line)
        };

        let (rename_from_line, line) = if line.starts_with(b"rename from ") {
            (
                Some(line),
                *lines
                    .next()
                    .with_context(|| format!("unexpected end of diff after line {line}"))?,
            )
        } else {
            (None, line)
        };

        let (rename_to_line, line) = if line.starts_with(b"rename to ") {
            (Some(line), lines.next().copied())
        } else {
            (None, Some(line))
        };

        let differences = if let Some(line) = line {
            let (index_line, line) = if line.starts_with(b"index ") {
                (
                    Some(line),
                    *lines
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

            let line = *lines
                .next()
                .with_context(|| "unexpected end of diff after line {line}")?;
            if !line.starts_with(b"+++ ") {
                bail!("invalid patch file format: expected `+++ ` on line {line}");
            }
            let plus_line = line;

            let hunks = gather_hunks(lines.as_slice(), bump);

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
            diff_path_a_full,
            diff_path_b_full,
            newfile_line,
            deleted_line,
            similarity_line,
            rename_from_line,
            rename_to_line,
            differences,
        })
    }
}
