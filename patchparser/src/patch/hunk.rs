use std::{io::Write, ops::Deref};

use anyhow::{bail, Context, Result};
use bumpalo::Bump;

use crate::{
    bumpalo_cow::{BumpaloCow, ToOwnedIn},
    line::{write_lines_to, Line},
    patch::{
        change::Change,
        change_line::{ChangeLineKind, ChangeLineReport, SeparateErrors},
    },
    re,
    reborrow_in::ReborrowIn,
    regex_utils::GetStr,
    utils::try_take_while,
};

pub trait WriteAsHunk {
    fn write_as_hunk_to(&self, out: impl Write) -> Result<(), std::io::Error>;
}

/// A group of lines starting with a "@@" line and not containing
/// other such lines; contains any number of changes
#[derive(Clone, PartialEq, Eq)]
pub struct Hunk<'a> {
    /// The "@@ " line
    pub head_line: Line<'a>,
    pub remaining_lines: BumpaloCow<'a, 'a, [Line<'a>]>,
}

impl<'a> WriteAsHunk for Hunk<'a> {
    fn write_as_hunk_to(&self, mut out: impl Write) -> Result<(), std::io::Error> {
        write_lines_to(&[self.head_line], &mut out)?;
        write_lines_to(&*self.remaining_lines, &mut out)
    }
}

impl<'a, 'b> ReborrowIn<'b> for Hunk<'a>
where
    'a: 'b,
{
    type Reborrowed = Hunk<'b>;

    fn reborrow_in(&self, _bump: &'b Bump) -> Hunk<'b>
    where
        'a: 'b,
    {
        Hunk {
            head_line: self.head_line,
            remaining_lines: BumpaloCow::Owned(self.remaining_lines.deref().to_owned_in(_bump)),
        }
    }
}

impl<'a> Hunk<'a> {
    // Just for testing
    #[allow(unused)]
    fn from_lines(head_line: Line<'a>, remaining_lines: BumpaloCow<'a, 'a, [Line<'a>]>) -> Self {
        Self {
            head_line,
            remaining_lines,
        }
    }

    pub fn split_into_changes<'h>(&'h self) -> Result<Vec<Change<'a, 'h>>> {
        let head_line = &self.head_line;

        // @@ -0,0 +1,2 @@
        // @@ -42 42 @@
        // @@ -42 +1,2 @@
        // @@ -0,0 +1 @@
        let caps = re!(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? (.*)")
            .captures(head_line)
            .with_context(|| format!("invalid hunk head on line {head_line}"))?;

        let orig_start: usize = caps.str_then_parse(1, *head_line)?;
        let orig_len: usize = caps.get_str_then_parse(2, *head_line)?.unwrap_or(1);
        let orig_patched_start: usize = caps.str_then_parse(3, *head_line)?;
        let orig_patched_len: usize = caps.get_str_then_parse(4, *head_line)?.unwrap_or(1);
        let head_post = caps.str(5);

        let mut remaining: &[Line] = &self.remaining_lines;
        let mut result = Vec::new();
        let mut start = orig_start;
        let mut patched_start = orig_patched_start;

        while !remaining.is_empty() {
            let (pre, after_pre, stop_reason) = try_take_while(remaining, |line| {
                ChangeLineReport::from(*line).matches_kinds(&[ChangeLineKind::Context])
            });
            stop_reason.separate_errors()?;
            let (group, after_group, stop_reason) = try_take_while(after_pre, |line| {
                ChangeLineReport::from(*line)
                    .matches_kinds(&[ChangeLineKind::Plus, ChangeLineKind::Minus])
            });
            stop_reason.separate_errors()?;
            let (post, rest, stop_reason) = try_take_while(after_group, |line| {
                ChangeLineReport::from(*line)
                    .matches_kinds(&[ChangeLineKind::Context, ChangeLineKind::Backslash])
            });
            let end_indicator = stop_reason.separate_errors()?;
            // Optional assertments:
            match end_indicator {
                Some(report) => match report {
                    ChangeLineReport::Kind(change_line_kind) => match change_line_kind {
                        ChangeLineKind::Plus | ChangeLineKind::Minus => "another group is fine",
                        ChangeLineKind::Context | ChangeLineKind::Backslash => {
                            unreachable!("those were taken above")
                        }
                    },
                    ChangeLineReport::Terminator(change_terminator) => unreachable!(
                        "buggy hunk creation: another {change_terminator:?} \
                         should not be possible within a hunk"
                    ),
                    ChangeLineReport::InvalidSyntax(_) => {
                        unreachable!("removed by `separate_errors`")
                    }
                },
                None => "end of input is fine",
            };

            let pre_len = pre.len();

            let new_pre = if pre.len() > 3 {
                &pre[pre.len() - 3..]
            } else {
                pre
            };
            let new_pre_len = new_pre.len();

            let new_post = if post.len() > 3 { &post[..3] } else { post };
            let new_post_len = new_post.len();

            let group_minus_len = group.iter().filter(|l| l.starts_with(b"-")).count();
            // The group consists purely of lines starting with '-' and
            // '+' by its construction, hence:
            let group_plus_len = group.len() - group_minus_len;

            let orig_len = new_pre_len + group_minus_len + new_post_len;
            let patched_len = new_pre_len + group_plus_len + new_post_len;

            result.push(Change {
                orig_start: start,
                orig_len,
                patched_start,
                patched_len,
                head_post,
                pre: new_pre,
                group,
                post: new_post,
            });

            // Note that `rest` is *not* the same value as the next
            // `remaining` value! We stop when there is no more groups
            // coming, not when there are no more context lines.

            start += pre_len + group_minus_len;
            patched_start += pre_len + group_plus_len;
            remaining = after_group;

            // XXX is it safe to compare `rest = [""]`?
            if rest.is_empty() || rest.iter().any(|l| l.is_empty()) {
                break;
            }
        }

        // "\ .. " lines are not included in the span lengths, thus
        // exclude them
        let remaining_active_count = remaining
            .iter()
            .filter(|line| {
                let report = ChangeLineReport::from(**line);
                !matches!(report, ChangeLineReport::Kind(ChangeLineKind::Backslash))
            })
            .count();
        // dbg!((remaining_active_count, remaining.len()));

        let actual_orig_len = start + remaining_active_count - orig_start;
        if orig_len != actual_orig_len {
            bail!(
                "hunk specified -{orig_start},{orig_len}, \
                 but the actual length of the hunk is {actual_orig_len} \
                 on line {head_line}"
            )
        }

        let actual_patched_len = patched_start + remaining_active_count - orig_patched_start;
        if orig_patched_len != actual_patched_len {
            bail!(
                "hunk specified +{orig_patched_start},{orig_patched_len}, \
                 but the actual patched length of the hunk is {actual_patched_len} \
                 on line {head_line}"
            )
        }

        Ok(result)
    }
}

#[test]
fn t_split_hunk_into_changes() {
    use bumpalo::{
        collections::{self as bc, CollectIn},
        Bump,
    };

    fn l<'a>(line0: usize, s: &'a str) -> Line<'a> {
        Line::from_tuple((line0, s.as_ref()))
    }

    let bump = Bump::new();

    let hunk_str = r#"
@@ -550,11 +552,11 @@ fn cmp_function(
 }
 
 fn run_processing_commands<'t: 'u, 'u: 'v, 'v>(
-    items: &'v mut Vec<Item<'t>>,
+    items: &'v mut Vec<Item<'t, &'t Path>>,
     cmds: &[ProcessingCommand],
     now: SystemTime,
     show_files_from_future: bool,
-) -> &'v [Item<'t>] {
+) -> &'v [Item<'t, &'t Path>] {
     probe!("run_processing_commands");
     let mut selected_items = un_safe { hack_static(&mut **items) };
     for cmd in cmds {
"#;
    let lines: bc::Vec<_> = hunk_str
        .trim()
        .split("\n")
        .enumerate()
        .map(|(i, line)| Line::from_tuple((i, line.as_ref())))
        .collect_in(&bump);
    let hunk = Hunk::from_lines(lines[0], BumpaloCow::Borrowed(&lines[1..]));
    let changes = hunk.split_into_changes().unwrap();

    let expected_changes = [
        Change {
            orig_start: 550,
            orig_len: 7,
            patched_start: 552,
            patched_len: 7,
            head_post: b"@@ fn cmp_function(",
            pre: &[
                l(1, " }"),
                l(2, " "),
                l(3, " fn run_processing_commands<'t: 'u, 'u: 'v, 'v>("),
            ],
            group: &[
                l(4, "-    items: &'v mut Vec<Item<'t>>,"),
                l(5, "+    items: &'v mut Vec<Item<'t, &'t Path>>,"),
            ],
            post: &[
                l(6, "     cmds: &[ProcessingCommand],"),
                l(7, "     now: SystemTime,"),
                l(8, "     show_files_from_future: bool,"),
            ],
        },
        Change {
            orig_start: 554,
            orig_len: 7,
            patched_start: 556,
            patched_len: 7,
            head_post: b"@@ fn cmp_function(",
            pre: &[
                l(6, "     cmds: &[ProcessingCommand],"),
                l(7, "     now: SystemTime,"),
                l(8, "     show_files_from_future: bool,"),
            ],
            group: &[
                l(9, "-) -> &'v [Item<'t>] {"),
                l(10, "+) -> &'v [Item<'t, &'t Path>] {"),
            ],
            post: &[
                l(11, "     probe!(\"run_processing_commands\");"),
                l(
                    12,
                    "     let mut selected_items = un_safe { hack_static(&mut **items) };",
                ),
                l(13, "     for cmd in cmds {"),
            ],
        },
    ];
    assert_eq!(changes, expected_changes);
}
