use std::io::Write;

use crate::{
    line::{write_lines_to, Line},
    patch::hunk::WriteAsHunk,
};

/// A single group of "-" and "+" lines and context around them; a
/// number of changes make up a hunk
#[derive(Debug, PartialEq, Eq)]
pub struct Change<'a, 'h> {
    pub orig_start: usize,
    pub orig_len: usize,
    pub patched_start: usize,
    pub patched_len: usize,
    pub head_post: &'a [u8],
    pub pre: &'h [Line<'a>],
    pub group: &'h [Line<'a>],
    pub post: &'h [Line<'a>],
}

impl<'a, 'h> WriteAsHunk for Change<'a, 'h> {
    fn write_as_hunk_to(&self, mut out: impl Write) -> Result<(), std::io::Error> {
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
        // XXX: Does the header need to be adapted to the following
        // patterns? As discovered for `split_hunk`
        // @@ -0,0 +1,2 @@
        // @@ -42 42 @@
        // @@ -42 +1,2 @@
        // @@ -0,0 +1 @@
        write!(
            &mut out,
            "@@ -{},{} +{},{} ",
            orig_start, orig_len, patched_start, patched_len
        )?;
        out.write_all(head_post)?;
        out.write_all(b"\n")?;
        write_lines_to(*pre, &mut out)?;
        write_lines_to(*group, &mut out)?;
        write_lines_to(*post, &mut out)?;
        Ok(())
    }
}
