use std::{io::Write, ops::Deref};

use anyhow::{bail, Context, Result};
use bstr::{BStr, BString, ByteSlice};
use bumpalo::{
    collections::{self as bc, CollectIn},
    Bump,
};

use crate::{
    bumpalo_bstring as b,
    bumpalo_cow::{BumpaloCow, ToOwnedIn},
    from_lines::FromLines,
    line::{write_lines_to, Line},
    patch::diff::Diff,
    reborrow_in::ReborrowIn,
    utils::split_before,
    write_to::WriteTo,
};

/// ASCII-case insensitive string comparison
pub fn string_equal_ci<A: AsRef<[u8]>, B: AsRef<[u8]>>(a: A, b: B) -> bool {
    let a = a.as_ref();
    let b = b.as_ref();
    a.len() == b.len() && {
        for (ac, bc) in a.iter().zip(b) {
            if !ac.eq_ignore_ascii_case(bc) {
                return false;
            }
        }
        true
    }
}

#[test]
fn t_ci_eq() {
    use string_equal_ci as eq;
    assert!(eq(b"", b""));
    assert!(!eq(b"a", b""));
    assert!(eq(b"a", b"a"));
    assert!(eq(b"a", b"A"));
    assert!(!eq(b"a", b"B"));
    assert!(!eq(b"a", b"b"));
    assert!(!eq(b"a", b"aa"));
    assert!(eq(b"aB", b"Ab"));
    assert!(eq(b"a1", b"A1"));
    assert!(!eq(b"a1", b"A2"));
}

struct HeaderLine<'a> {
    mixed_case_header_name: &'a [u8],
    // b": " or b":"
    separator: &'static [u8],
    value: &'a [u8],
}

impl<'a> HeaderLine<'a> {
    fn from_line(line: Line<'a>) -> Option<HeaderLine<'a>> {
        if let Some((mixed_case_header_name, rest)) = line.split_once_str(b":") {
            if mixed_case_header_name.iter().copied().all(is_key_char) {
                // Only use the part after the first space after the ":"
                let (separator, value) = if rest.starts_with(b" ") {
                    (bstr::B(": "), &rest[1..])
                } else {
                    (bstr::B(":"), rest)
                };
                return Some(Self {
                    mixed_case_header_name,
                    separator,
                    value,
                });
            }
        }
        None
    }
}

/// `git format-patch` style files have a "From " line and then a
/// number of header lines, before an empty line and body lines
/// follow; this represents this part before the empty line.
#[derive(Clone, PartialEq, Eq)]
pub struct PatchHeadHeader<'a> {
    pub from_line: Line<'a>,
    // Owning *here* is the means for *repeated* mutation without
    // copying again into a new slice. Thus use Cow over fresh
    // allocations from bumpalo (locally-generated line contents are
    // still owned by bumpalo).
    pub header_lines: BumpaloCow<'a, 'a, [Line<'a>]>,
}

pub fn is_key_char(b: u8) -> bool {
    match b {
        b'-' | b'_' => true,
        _ => b.is_ascii_alphanumeric(),
    }
}

impl<'a> WriteTo for PatchHeadHeader<'a> {
    fn write_to(&self, mut out: impl Write) -> Result<(), std::io::Error> {
        write_lines_to(&[self.from_line], &mut out)?;
        write_lines_to(&*self.header_lines, &mut out)
    }
}

impl<'a> FromLines<'a> for PatchHeadHeader<'a> {
    fn from_lines(lines: &'a [Line<'a>], _bump: &Bump) -> Result<Self, anyhow::Error> {
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

impl<'a, 'b> ReborrowIn<'b> for PatchHeadHeader<'a>
where
    'a: 'b,
{
    type Reborrowed = PatchHeadHeader<'b>;

    fn reborrow_in(&self, bump: &'b Bump) -> PatchHeadHeader<'b>
    where
        'a: 'b,
    {
        let Self {
            from_line,
            header_lines,
        } = self;
        PatchHeadHeader {
            from_line: *from_line,
            header_lines: BumpaloCow::Owned(header_lines.deref().to_owned_in(bump)),
        }
    }
}

impl<'a> PatchHeadHeader<'a> {
    /// Returns Self and the rest after the header if there is one
    pub fn _from_lines(lines: &'a [Line<'a>]) -> Option<(Self, &'a [Line<'a>])> {
        if let Some(from_line) = lines.first().copied() {
            if from_line.starts_with(b"From ") {
                if let Some(i) = lines.iter().position(|line| line.is_empty()) {
                    let header_lines = (&lines[1..i]).into();
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
                            header_lines: lines.into(),
                        },
                        &[],
                    ));
                }
            }
        }
        None
    }

    /// `header_name` is case insensitive. `f` is called with the
    /// remainder after the key, colon and optional first space, if a
    /// header with that name is found, and when it returns a value,
    /// that value is used to replace the part that was passed in.
    ///
    /// Returns the old value when the header was updated.
    ///
    /// The new line contents is allocated from `allocator`.
    pub fn update_header(
        &mut self,
        header_name: impl AsRef<BStr>,
        mut f: impl FnMut(&'a BStr) -> Option<BString>,
        bump: &'a Bump,
    ) -> Option<&'a BStr> {
        let header_name = header_name.as_ref();
        for line in self.header_lines.to_mut(bump) {
            if let Some(header_line) = HeaderLine::from_line(*line) {
                if string_equal_ci(header_line.mixed_case_header_name, header_name) {
                    if let Some(replacement) = f(header_line.value.as_ref()) {
                        let mut contents = b::BString::new_in(bump);
                        contents.extend_from_slice(header_line.mixed_case_header_name);
                        contents.extend_from_slice(header_line.separator);
                        contents.extend_from_slice(&replacement);
                        line.set_contents(contents.into_bump_slice());
                        return Some(header_line.value.as_ref());
                    }
                }
            }
        }
        None
    }

    /// `f` is called for all header lines, currently only the first
    /// one with the key (i.e. continuation lines are not passed to
    /// `f` and instead always left unchanged); it receives the header
    /// name (without the colon), the remainder of the line (after the
    /// colon and optionally a single space), as well as the whole
    /// original line. If it returns a value, then it is used as the
    /// value after the (original) "key: "; if it returns None, the
    /// header remains unchanged.
    pub fn header_mapped_write_to(
        &self,
        mut f: impl FnMut(&BStr, &BStr, Line<'a>) -> Option<BString>,
        mut out: impl Write,
    ) -> Result<(), std::io::Error> {
        write_lines_to(&[self.from_line], &mut out)?;
        for line in &*self.header_lines {
            if let Some(header_line) = HeaderLine::from_line(*line) {
                if let Some(replacement) = f(
                    BStr::new(header_line.mixed_case_header_name),
                    BStr::new(header_line.value),
                    *line,
                ) {
                    out.write_all(header_line.mixed_case_header_name)?;
                    out.write_all(header_line.separator)?;
                    out.write_all(&replacement)?;
                    out.write_all(b"\n")?;
                    continue;
                }
            }
            write_lines_to(&[*line], &mut out)?;
        }
        Ok(())
    }
}

/// The part before the first `diff ` line; can be empty
#[derive(Clone, PartialEq, Eq)]
pub struct PatchHead<'a> {
    pub header: Option<PatchHeadHeader<'a>>,
    /// If a header is given, remaining_lines starts with the empty
    /// line that follows the header. If no header was found, this
    /// holds all the lines found.
    pub remaining_lines: &'a [Line<'a>],
}

impl<'a, 'b> ReborrowIn<'b> for PatchHead<'a>
where
    'a: 'b,
{
    type Reborrowed = PatchHead<'b>;

    fn reborrow_in(&self, bump: &'b Bump) -> PatchHead<'b> {
        let Self {
            header,
            remaining_lines,
        } = self;
        PatchHead {
            header: header.as_ref().map(|h| h.reborrow_in(bump)),
            remaining_lines,
        }
    }
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

    pub fn update_header(
        &mut self,
        header_name: impl AsRef<BStr>,
        f: impl FnMut(&'a BStr) -> Option<BString>,
        allocator: &'a Bump,
    ) -> Option<&'a BStr> {
        if let Some(header) = &mut self.header {
            header.update_header(header_name, f, allocator)
        } else {
            None
        }
    }

    pub fn header_mapped_write_to(
        &self,
        f: impl FnMut(&BStr, &BStr, Line<'a>) -> Option<BString>,
        mut out: impl Write,
    ) -> Result<(), std::io::Error> {
        if let Some(header) = &self.header {
            header.header_mapped_write_to(f, &mut out)?;
        }
        write_lines_to(self.remaining_lines, &mut out)
    }
}

impl<'a> WriteTo for PatchHead<'a> {
    fn write_to(&self, mut out: impl Write) -> Result<(), std::io::Error> {
        if let Some(header) = &self.header {
            header.write_to(&mut out)?;
        }
        write_lines_to(self.remaining_lines, &mut out)
    }
}

impl<'a> FromLines<'a> for PatchHead<'a> {
    fn from_lines(lines: &'a [Line<'a>], _bump: &Bump) -> Result<Self, anyhow::Error> {
        Ok(Self::_from_lines(lines))
    }
}

/// Parsed representation for a whole patch file (as per `git
/// format-patch`, but should parse files from other files like `diff
/// -u`, too)
#[derive(Clone, PartialEq, Eq)]
pub struct Patch<'a> {
    /// The head represents the lines found before the first "diff "
    /// line.
    pub head: PatchHead<'a>,
    pub diffs: bc::Vec<'a, Diff<'a>>,
    /// The lines from "-- " onwards in "git format-patch" style
    /// files, including the "-- " line.
    pub footer: &'a [Line<'a>],
}

impl<'a> FromLines<'a> for Patch<'a> {
    fn from_lines(lines: &'a [Line<'a>], bump: &'a Bump) -> Result<Self> {
        // Split off the footer, if any
        let (lines, footer) = if let Some(rev_i) = lines
            .iter()
            .rev()
            .position(|line| line.contents() == bstr::B("-- "))
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
                (chunks[0], &chunks[1..])
            };
        if diff_lines_groups.is_empty() {
            // bail!("file does not appear to contain any diffs");
            // XX should we accept that?
        }

        let head = PatchHead::_from_lines(head_lines);

        // Parse the diffs
        let diffs = diff_lines_groups
            .iter()
            .enumerate()
            .map(|(diff_i, diff_lines)| -> Result<_> {
                Diff::from_lines(diff_lines, bump).with_context(|| {
                    format!(
                        "parsing diff no. {}/{}",
                        diff_i + 1,
                        diff_lines_groups.len()
                    )
                })
            })
            .collect_in::<Result<_>>(bump)?;

        Ok(Patch {
            head,
            diffs,
            footer,
        })
    }
}
