use std::{io::Write, path::Path};

use anyhow::{bail, Context, Result};
use bstr::ByteSlice;
use ouroboros::self_referencing;

use crate::{
    line::{write_lines_to, Line},
    patch::diff::Diff,
    utils::split_before,
};

/// `git format-patch` style files have a "From " line and then a
/// number of header lines, before an empty line and body lines
/// follow; this represents this part before the empty line.
pub struct PatchHeadHeader<'a> {
    pub from_line: Line<'a>,
    pub header_lines: &'a [Line<'a>],
}

impl<'a> PatchHeadHeader<'a> {
    pub fn write_to(&self, mut out: impl Write) -> Result<(), std::io::Error> {
        write_lines_to(&[self.from_line], &mut out)?;
        write_lines_to(self.header_lines, &mut out)
    }
}

pub struct PatchHead<'a> {
    pub header: Option<PatchHeadHeader<'a>>,
    /// If a header is given, remaining_lines starts with the empty
    /// line that follows the header. If no header was found, this
    /// holds all the lines found.
    pub remaining_lines: &'a [Line<'a>],
}

impl<'a> PatchHead<'a> {
    pub fn from_lines(lines: &'a [Line<'a>]) -> Self {
        if let Some(from_line) = lines.first().copied() {
            if from_line.starts_with(b"From ") {
                if let Some(i) = lines.iter().position(|line| line.is_empty()) {
                    let header_lines = &lines[1..i];
                    let remaining_lines = &lines[i..];
                    return PatchHead {
                        header: Some(PatchHeadHeader {
                            from_line,
                            header_lines,
                        }),
                        remaining_lines,
                    };
                } else {
                    return PatchHead {
                        header: Some(PatchHeadHeader {
                            from_line,
                            header_lines: lines,
                        }),
                        remaining_lines: &[],
                    };
                }
            }
        }
        PatchHead {
            header: None,
            remaining_lines: lines,
        }
    }

    pub fn write_to(&self, mut out: impl Write) -> Result<(), std::io::Error> {
        if let Some(header) = &self.header {
            header.write_to(&mut out)?;
        }
        write_lines_to(self.remaining_lines, &mut out)
    }
}

pub struct Patch<'a> {
    /// The head represents the lines found before the first "diff "
    /// line.
    pub head: PatchHead<'a>,
    pub diffs: Vec<Diff<'a>>,
    /// The lines from "-- " onwards in "git format-patch" style
    /// files, including the "-- " line.
    pub footer: &'a [Line<'a>],
}

impl<'a> Patch<'a> {
    pub fn from_lines(lines: &'a [Line<'a>]) -> Result<Self> {
        // Split off the footer, if any
        let (lines, footer) = if let Some(rev_i) = lines
            .iter()
            .rev()
            .position(|line| line.contents() == b"-- ")
        {
            let i = lines.len() - rev_i - 1;
            (&lines[0..i], &lines[i..])
        } else {
            (lines, [].as_slice())
        };

        // Split into head and diffs
        let is_diff_line = |line: &Line| line.starts_with(b"diff ");
        let chunks = split_before(lines, is_diff_line, |slice| slice);
        let (head_lines, diff_lines_groups): (&[Line], &[&[Line]]) =
            if chunks[0].first().map(is_diff_line).unwrap_or(false) {
                // No head
                (&[], &chunks)
            } else {
                // First part is head
                (&chunks[0], &chunks[1..])
            };
        if diff_lines_groups.is_empty() {
            // XXX should perhaps accept that!
            bail!("file does not appear to contain any diffs");
        }

        let head = PatchHead::from_lines(head_lines);

        // Parse the diffs
        let diffs = diff_lines_groups
            .iter()
            .enumerate()
            .map(|(diff_i, diff_lines)| -> Result<_> {
                Diff::from_lines(*diff_lines).with_context(|| {
                    format!(
                        "parsing diff no. {}/{}",
                        diff_i + 1,
                        diff_lines_groups.len()
                    )
                })
            })
            .collect::<Result<_>>()?;

        Ok(Patch {
            head,
            diffs,
            footer,
        })
    }
}

/// This is a wrapper around `Patch` that owns the content
#[self_referencing]
pub struct PatchFile {
    pub content: Vec<u8>,
    #[borrows(content)]
    #[covariant]
    pub lines: Vec<Line<'this>>,
    #[borrows(lines)]
    #[covariant]
    // Made public via explicit accessor below
    patch: Patch<'this>,
}

impl PatchFile {
    pub fn from_content(content: Vec<u8>) -> Result<Self> {
        PatchFile::try_new(
            content,
            |content| {
                let lines: Vec<Line> = content.lines().enumerate().map(Line::from_tuple).collect();
                Ok(lines)
            },
            |lines| Patch::from_lines(lines),
        )
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        Self::from_content(std::fs::read(path).context("reading file")?)
    }

    pub fn patch(&self) -> &Patch<'_> {
        self.borrow_patch()
    }
}
