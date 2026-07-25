use std::path::Path;

use anyhow::{bail, Context, Result};
use bstr::ByteSlice;
use ouroboros::self_referencing;

use crate::{line::Line, patch::diff::Diff, utils::split_before};

pub struct Patch<'a> {
    pub head: &'a [Line<'a>],
    pub diffs: Vec<Diff<'a>>,
    // The lines from "-- " in git format-patch files
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
        let (head, diffs): (&[Line], &[&[Line]]) =
            if chunks[0].first().map(is_diff_line).unwrap_or(false) {
                // No head
                (&[], &chunks)
            } else {
                // First part is head
                (&chunks[0], &chunks[1..])
            };
        if diffs.is_empty() {
            bail!("file does not appear to contain any diffs");
        }

        // Parse the diffs
        let diffs = diffs
            .iter()
            .enumerate()
            .map(|(diff_i, diff_lines)| -> Result<_> {
                Diff::from_lines(*diff_lines)
                    .with_context(|| format!("parsing diff no. {}/{}", diff_i + 1, diffs.len()))
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
