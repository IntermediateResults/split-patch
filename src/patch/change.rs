use std::io::Write;

use crate::{patch::hunk::WriteAsHunk, utils::write_lines_to};

/// A single group of "-" and "+" lines and context around them; a
/// number of changes make up a hunk
#[derive(Debug, PartialEq, Eq)]
pub struct Change<'a, 'h> {
    pub orig_start: usize,
    pub orig_len: usize,
    pub patched_start: usize,
    pub patched_len: usize,
    pub head_post: &'a str,
    pub pre: &'h [(usize, &'a str)],
    pub group: &'h [(usize, &'a str)],
    pub post: &'h [(usize, &'a str)],
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
        writeln!(
            &mut out,
            "@@ -{},{} +{},{} {}",
            orig_start, orig_len, patched_start, patched_len, head_post
        )?;
        write_lines_to(pre.iter().map(|(_, s)| *s), &mut out)?;
        write_lines_to(group.iter().map(|(_, s)| *s), &mut out)?;
        write_lines_to(post.iter().map(|(_, s)| *s), &mut out)?;
        Ok(())
    }
}
