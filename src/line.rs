use std::{
    fmt::Display,
    io::{Read, Write},
    ops::Deref,
    path::Path,
};

use anyhow::{Context, Result};
use bstr::ByteSlice;
use bumpalo::{
    collections::{self as bc, CollectIn},
    Bump,
};

use crate::bumpalo_cow::CloneIn;

/// A reference to a line string without the line ending and the line
/// number for location reporting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line<'a> {
    /// 0-based line number; MAX represents "generated" (no location)
    line_no0: usize,
    contents: &'a [u8],
}

impl<'a> Deref for Line<'a> {
    type Target = &'a [u8];

    fn deref(&self) -> &Self::Target {
        &self.contents
    }
}

// XX painful, really no way out?
impl<'a> CloneIn<'a> for Line<'a> {
    fn clone_in(&self, _bump: &'a bumpalo::Bump) -> Self {
        *self
    }
}

impl<'a> Display for Line<'a> {
    /// Display line number rather than the line contents
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(line_no) = self.line_no() {
            write!(f, "{line_no}")
        } else {
            f.write_str("(no location)")
        }
    }
}

impl<'a> Line<'a> {
    pub fn from_tuple((line_no0, contents): (usize, &'a [u8])) -> Self {
        Self { line_no0, contents }
    }

    pub fn from_generated_content(contents: &'a [u8]) -> Self {
        Self {
            line_no0: usize::MAX,
            contents,
        }
    }

    /// line contents without newline
    pub fn contents(&self) -> &'a [u8] {
        self.contents
    }

    pub fn set_contents(&mut self, contents: &'a [u8]) {
        self.contents = contents;
    }

    /// 0-based line number; None if the line was generated (has no location)
    pub fn line_no0(&self) -> Option<usize> {
        if self.line_no0 == usize::MAX {
            None
        } else {
            Some(self.line_no0)
        }
    }

    /// 1-based line number; None if the line was generated (has no location)
    pub fn line_no(&self) -> Option<usize> {
        self.line_no0().map(|no| no + 1)
    }
}

/// Write the line strings out with line endings added
pub fn write_lines_to<'a>(
    lines: impl IntoIterator<Item = &'a Line<'a>>,
    mut out: impl Write,
) -> Result<(), std::io::Error> {
    for line in lines {
        out.write_all(line)?;
        out.write_all(b"\n")?;
    }
    Ok(())
}

pub fn read_in<'b, P: AsRef<Path>>(path: P, bump: &'b Bump) -> Result<bc::Vec<'b, u8>> {
    let mut input = std::fs::File::open(path).context("opening file for reading")?;
    let len = input.metadata()?.len();
    let len_usize = usize::try_from(len).expect("file is too large");
    let mut contents = bc::Vec::<u8>::with_capacity_in(len_usize, bump);
    unsafe {
        // Safe because we'll never read from the bytes unless they
        // have been written to
        contents.set_len(len_usize);
    }

    input.read_exact(&mut contents)?;
    Ok(contents)
}

pub fn read_lines_in<'b, P: AsRef<Path>>(path: P, bump: &'b Bump) -> Result<bc::Vec<'b, Line<'b>>> {
    let contents = read_in(path, bump)?.into_bump_slice();
    Ok(contents
        .lines()
        .enumerate()
        .map(Line::from_tuple)
        .collect_in(bump))
}
