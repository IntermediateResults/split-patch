use std::{fmt::Display, io::Write};

use anyhow::{anyhow, Context, Result};
use bstr::{BStr, ByteSlice};
use bumpalo::{
    collections::{self as bc, CollectIn},
    Bump,
};

use crate::{
    bumpalo_bstring::BString,
    line::{write_lines_to, Line},
    patch::{
        change::{Change, Position},
        change_line::{ChangeLineKind, ChangeLineReport},
    },
    re,
    reborrow_in::ReborrowIn,
    regex_utils::GetStr,
    write_to::WriteTo,
};

// XX take as parameter instead
const MAX_CONTEXT_LEN: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FullHunkHead<'a> {
    pub orig_start: usize,
    pub orig_len: usize,
    pub patched_start: usize,
    pub patched_len: usize,
    pub head_post: Option<&'a BStr>,
}

impl<'a> FullHunkHead<'a> {
    pub fn from_line<'a0: 'a>(head_line: &Line<'a0>) -> Result<Self> {
        // @@ -0,0 +1,2 @@
        // @@ -42 42 @@
        // @@ -42 +1,2 @@
        // @@ -0,0 +1 @@
        let caps = re!(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))?(?: *@@(?: ?( *[^ ].*))?)$")
            .captures(head_line)
            .with_context(|| format!("invalid hunk head on line {head_line}"))?;

        let orig_start: usize = caps.str_then_parse(1, *head_line)?;
        let orig_len: usize = caps.get_str_then_parse(2, *head_line)?.unwrap_or(1);
        let patched_start: usize = caps.str_then_parse(3, *head_line)?;
        let patched_len: usize = caps.get_str_then_parse(4, *head_line)?.unwrap_or(1);
        let head_post = caps.get_str(5).map(ByteSlice::as_bstr);
        Ok(FullHunkHead {
            orig_start,
            orig_len,
            patched_start,
            patched_len,
            head_post,
        })
    }

    pub fn to_line(&self, bump: &'a Bump) -> Line<'a> {
        let Self {
            orig_start,
            orig_len,
            patched_start,
            patched_len,
            head_post,
        } = self;
        // Does the header need to be adapted to the following
        // patterns? As discovered for `split_hunk` -- cj: Let's just
        // always print the multi-line range format, it should always
        // work.
        // @@ -0,0 +1,2 @@
        // @@ -42 42 @@
        // @@ -42 +1,2 @@
        // @@ -0,0 +1 @@
        let mut content = BString::new_in(bump);
        content.extend_from_slice(
            format!(
                "@@ -{},{} +{},{} @@",
                orig_start, orig_len, patched_start, patched_len
            )
            .as_bytes(),
        );
        if let Some(head_post) = head_post {
            content.push(b' ');
            content.extend_from_slice(head_post);
        }
        Line::from_generated_content(content.into_bump_slice())
    }

    /// Also returns the `orig_len` and `patched_len` values
    pub fn to_minimal_hunk_head(&self) -> (MinimalHunkHead<'a>, (usize, usize)) {
        let FullHunkHead {
            orig_start,
            orig_len,
            patched_start,
            patched_len,
            head_post,
        } = self;
        (
            MinimalHunkHead {
                orig_start: *orig_start,
                patched_start: *patched_start,
                head_post: *head_post,
            },
            (*orig_len, *patched_len),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MinimalHunkHead<'a> {
    pub orig_start: usize,
    pub patched_start: usize,
    pub head_post: Option<&'a BStr>,
}

impl<'a> MinimalHunkHead<'a> {
    /// The start positions in original and patched version of the
    /// file
    pub fn starts(&self) -> (usize, usize) {
        let Self {
            orig_start,
            patched_start,
            head_post: _,
        } = self;
        (*orig_start, *patched_start)
    }
}

impl<'a> WriteTo for FullHunkHead<'a> {
    fn write_to(&self, out: impl Write) -> Result<(), std::io::Error> {
        // Temporary allocator, for one line with a possible
        // re-allocation; since those lines can have long `head_post`
        // strings, give it some leeway (XX no problem if this is too
        // small, it will allocate more, right?)
        let bump = Bump::with_capacity(300);
        write_lines_to(&[self.to_line(&bump)], out)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedHunk<'a> {
    pub head: MinimalHunkHead<'a>,
    // Change is pretty large thus store by reference for cheap re-use
    pub changes: &'a [&'a Change<'a>],
}

impl<'a> WriteTo for ParsedHunk<'a> {
    fn write_to(&self, mut out: impl Write) -> Result<(), std::io::Error> {
        let Self { head: _, changes } = self;
        let head = self.full_hunk_head();
        head.write_to(&mut out)?;
        let last_i = changes.len().wrapping_sub(1);
        for (i, change) in changes.iter().enumerate() {
            let position = Position {
                is_first: i == 0,
                is_last: i == last_i,
            };
            change.write_to(&mut out, position, MAX_CONTEXT_LEN)?;
        }
        Ok(())
    }
}

// XX do we still want that?
impl<'a, 'b> ReborrowIn<'b> for ParsedHunk<'a>
where
    'a: 'b,
{
    type Reborrowed = ParsedHunk<'b>;

    fn reborrow_in(&self, _bump: &'b Bump) -> Self::Reborrowed
    where
        'a: 'b,
    {
        let Self { head, changes } = self;
        ParsedHunk {
            head: head.clone(),
            changes,
        }
    }
}

#[derive(Debug)]
pub enum CheckError {
    InconsistentHunkHead(anyhow::Error),
}

// Avoid dependency on thiserror:

impl Display for CheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckError::InconsistentHunkHead(error) => write!(f, "{error:#}"),
        }
    }
}

impl std::error::Error for CheckError {}

impl<'a> ParsedHunk<'a> {
    pub fn from_lines(
        lines: &'a [Line<'a>],
        bump: &'a Bump,
        mut handle_check_error: impl FnMut(&dyn Fn() -> Result<(), CheckError>) -> Result<()>,
    ) -> Result<Self> {
        // (XX how was that with hunk-less diffs? Does it works out
        // OK? Add tests!)
        let (head_line, rest) = lines
            .split_first()
            .context("a hunk must consist of at least a header line")?;
        let parsed_head = FullHunkHead::from_line(head_line)?;
        let (head, _) = parsed_head.to_minimal_hunk_head();
        let changes = split_hunk_into_changes(rest, bump)?.into_bump_slice();
        let parsed_hunk = ParsedHunk { head, changes };

        let check = || -> Result<(), CheckError> {
            let expected_head = parsed_hunk.full_hunk_head();
            if parsed_head != expected_head {
                let bump = Bump::with_capacity(300);
                Err(CheckError::InconsistentHunkHead(anyhow!(
                    "hunk range information on line {head_line} is inconsistent with body, expected:\n\
                     {}",
                    expected_head.to_line(&bump).contents()
                )))
            } else {
                Ok(())
            }
        };
        handle_check_error(&check)?;

        Ok(parsed_hunk)
    }

    pub fn full_hunk_head(&self) -> FullHunkHead<'a> {
        let Self { head, changes } = self;
        let MinimalHunkHead {
            orig_start,
            patched_start,
            head_post,
        } = head.clone();
        let last_i = changes.len().wrapping_sub(1);
        let mut orig_len = 0;
        let mut patched_len = 0;
        for (i, change) in changes.iter().enumerate() {
            let position = Position {
                is_first: i == 0,
                is_last: i == last_i,
            };
            let (add_orig, add_patched) = change.additional_lengths(position, MAX_CONTEXT_LEN);
            orig_len += add_orig;
            patched_len += add_patched;
        }
        FullHunkHead {
            orig_start,
            orig_len,
            patched_start,
            patched_len,
            head_post,
        }
    }

    pub fn split_by_change<'h, 'b>(&'h self, bump: &'b Bump) -> bc::Vec<'b, ParsedHunk<'b>>
    where
        'a: 'b,
    {
        let mut head = self.head.clone();
        self.changes
            .iter()
            .map(|change| {
                let h = ParsedHunk {
                    head: head.clone(),
                    changes: bump.alloc([*change]),
                };
                let (add_orig, add_patched) = change.offsets_for_next_change();
                head.orig_start += add_orig;
                head.patched_start += add_patched;
                h
            })
            .collect_in(bump)
    }
}

fn split_hunk_into_changes<'a>(
    rest: &[Line<'a>],
    bump: &'a Bump,
) -> Result<bc::Vec<'a, &'a Change<'a>>> {
    let mut lines_with_report = rest.iter().map(|line| -> Result<_> {
        let report = ChangeLineReport::from(*line).kind_or_terminator()?;
        let (_first_char, rest) = line
            .split_first()
            .expect("empty line would give an error report, handled above");
        let line_rest = line.with_changed_contents(rest.as_bstr());
        Ok((report, line_rest))
    });

    let mut changes = bc::Vec::new_in(bump);
    let mut post_from_previous_change: &'a [Line<'a>] = &[];

    let mut i_report_line;
    macro_rules! advance {
        {} => {
            i_report_line = lines_with_report.next().transpose()?;
        }
    }
    advance!();

    // Changes
    loop {
        let mut pre = bc::Vec::new_in(bump);
        while let Some((report, line)) = i_report_line {
            match report.kind() {
                Some(ChangeLineKind::Context) => pre.push(line),
                _ => break,
            }
            advance!();
        }

        let mut minus = bc::Vec::new_in(bump);
        let mut plus = bc::Vec::new_in(bump);
        while let Some((report, line)) = i_report_line {
            match report.kind() {
                Some(ChangeLineKind::Minus) => minus.push(line),
                Some(ChangeLineKind::Plus) => plus.push(line),
                _ => break,
            }
            advance!();
        }

        let mut post = bc::Vec::new_in(bump);
        while let Some((report, line)) = i_report_line {
            match report.kind() {
                Some(ChangeLineKind::Context) => post.push(line),
                _ => break,
            }
            advance!();
        }

        // XXX check _line ?
        let backslash = if let Some((report, _line)) = i_report_line {
            match report.kind() {
                Some(ChangeLineKind::Backslash) => {
                    advance!();
                    true
                }
                _ => false,
            }
        } else {
            false
        };

        let change = &*bump.alloc(Change {
            post_from_previous_change,
            pre: pre.into_bump_slice(),
            minus: minus.into_bump_slice(),
            plus: plus.into_bump_slice(),
            post: post.into_bump_slice(),
            backslash,
        });
        post_from_previous_change = change.post;
        changes.push(change);

        // // XXX is it safe to compare `rest = [""]`?
        // if rest.is_empty() || rest.iter().any(|l| l.is_empty()) {
        //     break;
        // }
        if i_report_line.is_none() {
            // XXX but must check for wrong terminators and stuff?,
            // first. -- or those reported as errors already?
            break;
        }
    }

    // if let Some((i, (report, line)))= i_report_line {
    //     bail!("XXX unexpected content in hunk on line {line}: {report:?}")
    // }

    Ok(changes)
}
