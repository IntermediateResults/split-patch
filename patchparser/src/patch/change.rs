use std::io::Write;

use anyhow::Result;

use crate::{
    line::{write_line_to, Line},
    patch::change_line::ChangeLineKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub is_first: bool,
    pub is_last: bool,
}

impl Position {
    pub const SOLE: Position = Position {
        is_first: true,
        is_last: true,
    };
}

/// A single group of "-" and "+" lines and context around them (part
/// of a `Hunk`).
///
/// Note that the `Line` instances here are *not* the full original
/// lines, but stripped of the first `ChangeLineKind`-determining
/// character (still using `Line` and not `BStr` since keeping the
/// line number is still valuable).
///
/// `pre` only holds *additional* lines *after* the `post` lines of a
/// previous change (if any)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change<'a> {
    pub post_from_previous_change: &'a [Line<'a>],
    pub pre: &'a [Line<'a>],
    pub minus: &'a [Line<'a>],
    pub plus: &'a [Line<'a>],
    pub post: &'a [Line<'a>],
    pub backslash: bool,
}

impl<'a> Change<'a> {
    /// The number of lines in addition to the (post-context of a)
    /// potential previous change (backslash line is excluded, as it
    /// is not counted in span lengths), for original and patched span
    /// lengths. Except if `position.is_first` is true, then the
    /// `post_from_previous_change` lines are counted, too. Only max
    /// `max_context_len` context lines are counted for sides that are
    /// at the end of the change series (between changes, all change
    /// lines are used).
    pub fn additional_lengths(&self, position: Position, max_context_len: usize) -> (usize, usize) {
        let max_len_pre = if position.is_first {
            max_context_len
        } else {
            usize::MAX
        };
        let max_len_post = if position.is_last {
            max_context_len
        } else {
            usize::MAX
        };

        let initial = if position.is_first {
            self.post_from_previous_change.len()
        } else {
            0
        };
        let context =
            (self.pre.len() + initial).min(max_len_pre) + self.post.len().min(max_len_post);
        (context + self.minus.len(), context + self.plus.len())
    }

    /// The number of lines this change takes up excluding its `post`
    /// context, i.e. what is needed to calculate the file start
    /// positions (original and patched) for the change that follows
    /// this one
    pub fn offsets_for_next_change(&self) -> (usize, usize) {
        let pre_context = self.post_from_previous_change.len() + self.pre.len();
        (
            pre_context + self.minus.len(),
            pre_context + self.plus.len(),
        )
    }

    /// Can't implement WriteTo trait as `position` argument is
    /// needed.
    pub fn write_to(
        &self,
        mut out: impl Write,
        position: Position,
        max_context_len: usize,
    ) -> Result<(), std::io::Error> {
        let Self {
            post_from_previous_change,
            pre,
            minus,
            plus,
            post,
            backslash,
        } = self;

        let max_len_pre = if position.is_first {
            max_context_len
        } else {
            usize::MAX
        };
        let max_len_post = if position.is_last {
            max_context_len
        } else {
            usize::MAX
        };

        let used_prevpost: &[Line];
        let used_pre: &[Line];
        if pre.len() >= max_len_pre {
            // all satisfied via pre
            used_prevpost = &[];
            used_pre = &pre[pre.len().saturating_sub(max_len_pre)..];
        } else {
            // pre is too short or equal, take all of it
            used_pre = *pre;
            if position.is_first {
                let still_need = max_len_pre - pre.len();
                used_prevpost = &post_from_previous_change
                    [post_from_previous_change.len().saturating_sub(still_need)..];
            } else {
                used_prevpost = &[];
            }
        }
        write_lines(ChangeLineKind::Context, used_prevpost, &mut out)?;
        write_lines(ChangeLineKind::Context, used_pre, &mut out)?;

        write_lines(ChangeLineKind::Minus, minus, &mut out)?;
        write_lines(ChangeLineKind::Plus, plus, &mut out)?;

        let used_post = &post[0..max_len_post.min(post.len())];
        write_lines(ChangeLineKind::Context, used_post, &mut out)?;

        if *backslash {
            out.write_all(b"\\ No newline at end of file\n")?;
        }
        Ok(())
    }
}

fn write_lines<'a>(
    kind: ChangeLineKind,
    lines: &[Line<'a>],
    mut out: impl Write,
) -> Result<(), std::io::Error> {
    let prefix = kind.prefix();
    for line in lines {
        out.write_all(&[prefix])?;
        write_line_to(line, &mut out)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use bstr::{BString, ByteSlice};

    use super::*;

    #[test]
    fn t_() -> Result<()> {
        let lines: Vec<Line> = [
            b"line 0", b"line 1", b"line 2", b"line 3", b"line 4", b"line 5", b"line 6", b"line 7",
            b"line 8", b"line 9",
        ]
        .iter()
        .enumerate()
        .map(|(i, s)| Line::from_lineno0_bstr(i, s.as_bstr()))
        .collect();

        let c1 = Change {
            post_from_previous_change: &[],
            pre: &lines[0..3],
            minus: &lines[3..4],
            plus: &lines[4..5],
            post: &lines[5..8],
            backslash: false,
        };

        let t = |c: &Change, position: Position, max_context_len: usize| {
            let mut out = Vec::new();
            c.write_to(&mut out, position, max_context_len).unwrap();
            BString::from(out)
        };
        let b = |s: &str| BString::from(&s[1..]);

        for position in [
            Position {
                is_first: true,
                is_last: true,
            },
            Position {
                is_first: false,
                is_last: true,
            },
        ] {
            dbg!(position);
            assert_eq!(
                t(&c1, position, 3),
                b("
 line 0
 line 1
 line 2
-line 3
+line 4
 line 5
 line 6
 line 7
")
            );
        }

        assert_eq!(
            t(&c1, Position::SOLE, 2),
            b("
 line 1
 line 2
-line 3
+line 4
 line 5
 line 6
")
        );
        assert_eq!(
            t(&c1, Position::SOLE, 1),
            b("
 line 2
-line 3
+line 4
 line 5
")
        );
        assert_eq!(
            t(&c1, Position::SOLE, 0),
            b("
-line 3
+line 4
")
        );

        let c2 = Change {
            post_from_previous_change: &lines[0..4],
            pre: &[],
            minus: &lines[4..6],
            plus: &lines[6..7],
            post: &lines[7..],
            backslash: false,
        };
        let c3 = Change {
            post_from_previous_change: &lines[0..2],
            pre: &lines[2..4],
            minus: &lines[4..6],
            plus: &lines[6..7],
            post: &lines[7..],
            backslash: false,
        };

        for c in [&c2, &c3] {
            dbg!(c);
            for ctx in [4, 5] {
                dbg!(ctx);
                assert_eq!(
                    t(c, Position::SOLE, ctx),
                    b("
 line 0
 line 1
 line 2
 line 3
-line 4
-line 5
+line 6
 line 7
 line 8
 line 9
")
                );
            }
            assert_eq!(
                t(c, Position::SOLE, 3),
                b("
 line 1
 line 2
 line 3
-line 4
-line 5
+line 6
 line 7
 line 8
 line 9
")
            );
            assert_eq!(
                t(c, Position::SOLE, 2),
                b("
 line 2
 line 3
-line 4
-line 5
+line 6
 line 7
 line 8
")
            );
            assert_eq!(
                t(c, Position::SOLE, 1),
                b("
 line 3
-line 4
-line 5
+line 6
 line 7
")
            );
            assert_eq!(
                t(c, Position::SOLE, 0),
                b("
-line 4
-line 5
+line 6
")
            );
        }

        Ok(())
    }
}
