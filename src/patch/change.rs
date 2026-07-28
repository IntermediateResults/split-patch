use std::{borrow::Cow, io::Write};

use bstr::{BString, ByteVec};
use bumpalo::Bump;

use crate::{
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
        let mut lines = Vec::new();

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
        let mut content = BString::new(
            format!(
                "@@ -{},{} +{},{} ",
                orig_start, orig_len, patched_start, patched_len
            )
            .into(),
        );
        content.push_str(head_post);
        // XX leak? Use Vec<u8> on bumpalo instead?
        lines.push(Line::from_generated_content(bump.alloc(content)));

        lines.extend_from_slice(pre);
        lines.extend_from_slice(group);
        lines.extend_from_slice(post);

        Hunk {
            lines: Cow::Owned(lines),
        }
    }
}

impl<'a, 'h> WriteAsHunk for Change<'a, 'h> {
    // XX obsolete and costlier than it used to be, pointless?
    fn write_as_hunk_to(&self, out: impl Write) -> Result<(), std::io::Error> {
        // XX a little costly
        let bump = Bump::new();
        self.to_hunk(&bump).write_as_hunk_to(out)
    }
}
