use std::io::Write;

use anyhow::{Context, Result};

use crate::{
    line::{write_lines_to, Line},
    patch::change::Change,
    re,
    re::GetStr,
    utils::take_while,
};

pub trait WriteAsHunk {
    fn write_as_hunk_to(&self, out: impl Write) -> Result<(), std::io::Error>;
}

/// A group of lines starting with a "@@" line and not containing
/// other such lines; contains any number of changes
#[derive(PartialEq, Eq)]
pub struct Hunk<'a> {
    /// The Vec is never empty, at least the "@@ " line is ensured by
    /// construction via `split_before` which does not create a group
    /// out of no lines.
    pub lines: &'a [Line<'a>],
}

impl<'a> WriteAsHunk for Hunk<'a> {
    fn write_as_hunk_to(&self, out: impl Write) -> Result<(), std::io::Error> {
        write_lines_to(self.lines, out)
    }
}

impl<'a> Hunk<'a> {
    // Just for testing
    #[allow(unused)]
    fn from_lines(lines: &'a [Line<'a>]) -> Self {
        Self { lines }
    }

    pub fn split_into_changes<'h>(&'h self) -> Result<Vec<Change<'a, 'h>>> {
        let head_line = self
            .lines
            .first()
            .expect("hunks are expected to never be empty by construction");

        // XXX: are all these valid patterns? What happens if captures
        // fail?
        // @@ -0,0 +1,2 @@
        // @@ -42 42 @@
        // @@ -42 +1,2 @@
        // @@ -0,0 +1 @@
        let caps = re!(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? (.*)")
            .captures(head_line)
            .with_context(|| format!("invalid hunk head: {head_line}"))?;

        let mut orig_start: usize = caps.get_str_then_parse(1, head_line.line_no0())?;
        let mut patched_start: usize = caps.get_str_then_parse(3, head_line.line_no0())?;
        let head_post = caps.get_str(5);

        let mut remaining: &[Line] = &self.lines[1..];
        let mut result = Vec::new();

        fn starts_with_space_or_backslash(l: &Line) -> bool {
            match l.first() {
                Some(b' ') | Some(b'\\') => true,
                _ => false,
            }
        }
        fn starts_with_minus_or_plus(l: &Line) -> bool {
            match l.first() {
                Some(b'-') | Some(b'+') => true,
                _ => false,
            }
        }

        while !remaining.is_empty() {
            let (pre, after_pre) = take_while(remaining, starts_with_space_or_backslash);
            let (group, after_group) = take_while(after_pre, starts_with_minus_or_plus);
            let (post, rest) = take_while(after_group, starts_with_space_or_backslash);

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
                orig_start,
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

            // XXX is it safe to compare `rest = [""]`?
            if rest.is_empty() || rest.iter().any(|l| l.is_empty()) {
                break;
            }

            orig_start += pre_len + group_minus_len;
            patched_start += pre_len + group_plus_len;
            remaining = after_group;
        }

        Ok(result)
    }
}

#[test]
fn t_split_hunk_into_changes() {
    fn l<'a>(line0: usize, s: &'a str) -> Line<'a> {
        Line::from_tuple((line0, s.as_ref()))
    }

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
     let mut selected_items = unsafe { hack_static(&mut **items) };
     for cmd in cmds {
"#;
    let lines: Vec<_> = hunk_str
        .trim()
        .split("\n")
        .enumerate()
        .map(|(i, line)| Line::from_tuple((i, line.as_ref())))
        .collect();
    let hunk = Hunk::from_lines(&lines);
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
                    "     let mut selected_items = unsafe { hack_static(&mut **items) };",
                ),
                l(13, "     for cmd in cmds {"),
            ],
        },
    ];
    assert_eq!(changes, expected_changes);
}
