use std::io::Write;

use bumpalo::{collections as bc, Bump};

use crate::{
    bumpalo_bstring::BString,
    bumpalo_cow::BumpaloCow,
    line::Line,
    patch::hunk::{Hunk, WriteAsHunk},
};

/// A single group of "-" and "+" lines and context around them; a
/// number of changes make up a hunk
#[derive(Debug, PartialEq, Eq)]
pub struct Change<'a, 'h> {
    /// Info for the first line
    pub orig_start: usize,
    pub orig_len: usize,
    pub patched_start: usize,
    pub patched_len: usize,
    pub head_post: &'a [u8],
    /// Remaining lines
    pub pre: &'h [Line<'a>],
    pub group: &'h [Line<'a>],
    pub post: &'h [Line<'a>],
}

impl<'a, 'h> Change<'a, 'h> {
    pub fn to_hunk<'b>(&self, bump: &'b Bump) -> Hunk<'b>
    where
        'a: 'b,
        'h: 'b,
    {
        let mut lines = bc::Vec::new_in(bump);

        let Self {
            orig_start,
            orig_len,
            patched_start,
            patched_len,
            head_post,
            pre,
            group,
            post,
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
                "@@ -{},{} +{},{} ",
                orig_start, orig_len, patched_start, patched_len
            )
            .as_bytes(),
        );
        content.extend_from_slice(head_post);
        lines.push(Line::from_generated_content(content.into_bump_slice()));

        lines.extend_from_slice(pre);
        lines.extend_from_slice(group);
        lines.extend_from_slice(post);

        Hunk {
            lines: BumpaloCow::Owned(lines),
        }
    }
}

impl<'a, 'h> WriteAsHunk for Change<'a, 'h> {
    // XX obsolete and costlier than it used to be, pointless?
    fn write_as_hunk_to(&self, out: impl Write) -> Result<(), std::io::Error> {
        // XX a little costly
        let bump = Bump::new();
        let hunk = self.to_hunk(&bump);
        hunk.write_as_hunk_to(out)
    }
}
