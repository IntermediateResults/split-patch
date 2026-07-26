use std::io::Write;

use anyhow::{bail, Context, Result};

use crate::{
    def_line_content_for,
    line::{write_lines_to, Line},
    line_content::FromLines,
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

impl<'a> FromLines<'a> for PatchHeadHeader<'a> {
    fn from_lines(lines: &'a [Line<'a>]) -> Result<Self, anyhow::Error> {
        if let Some((header, rest)) = Self::_from_lines(lines) {
            if rest.is_empty() {
                return Ok(header);
            }
            bail!(
                "the given lines contain a patch head header, but also more lines: {}",
                rest[0]
            )
        }
        bail!("the given lines do not represent a patch head header")
    }
}

def_line_content_for!(OwnedPatchHeadHeader, PatchHeadHeader);

impl<'a> PatchHeadHeader<'a> {
    pub fn write_to(&self, mut out: impl Write) -> Result<(), std::io::Error> {
        write_lines_to(&[self.from_line], &mut out)?;
        write_lines_to(self.header_lines, &mut out)
    }

    /// Returns Self and the rest after the header if there is one
    pub fn _from_lines(lines: &'a [Line<'a>]) -> Option<(Self, &'a [Line<'a>])> {
        if let Some(from_line) = lines.first().copied() {
            if from_line.starts_with(b"From ") {
                if let Some(i) = lines.iter().position(|line| line.is_empty()) {
                    let header_lines = &lines[1..i];
                    let remaining_lines = &lines[i..];
                    return Some((
                        PatchHeadHeader {
                            from_line,
                            header_lines,
                        },
                        remaining_lines,
                    ));
                } else {
                    return Some((
                        PatchHeadHeader {
                            from_line,
                            header_lines: lines,
                        },
                        &[],
                    ));
                }
            }
        }
        None
    }
}

/// The part before the first `diff ` line; can be empty
pub struct PatchHead<'a> {
    pub header: Option<PatchHeadHeader<'a>>,
    /// If a header is given, remaining_lines starts with the empty
    /// line that follows the header. If no header was found, this
    /// holds all the lines found.
    pub remaining_lines: &'a [Line<'a>],
}

impl<'a> PatchHead<'a> {
    pub fn _from_lines(lines: &'a [Line<'a>]) -> Self {
        if let Some((header, remaining_lines)) = PatchHeadHeader::_from_lines(lines) {
            return PatchHead {
                header: Some(header),
                remaining_lines,
            };
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

impl<'a> FromLines<'a> for PatchHead<'a> {
    fn from_lines(lines: &'a [Line<'a>]) -> Result<Self, anyhow::Error> {
        Ok(Self::_from_lines(lines))
    }
}

def_line_content_for!(OwnedPatchHead, PatchHead);

/// Parsed representation for a whole patch file (as per `git
/// format-patch`, but should parse files from other files like `diff
/// -u`, too)
pub struct Patch<'a> {
    /// The head represents the lines found before the first "diff "
    /// line.
    pub head: PatchHead<'a>,
    pub diffs: Vec<Diff<'a>>,
    /// The lines from "-- " onwards in "git format-patch" style
    /// files, including the "-- " line.
    pub footer: &'a [Line<'a>],
}

impl<'a> FromLines<'a> for Patch<'a> {
    fn from_lines(lines: &'a [Line<'a>]) -> Result<Self> {
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

        let head = PatchHead::_from_lines(head_lines);

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

def_line_content_for!(OwnedPatch, Patch);
