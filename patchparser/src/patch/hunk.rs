use std::{borrow::Cow, io::Write};

use anyhow::Result;
use bumpalo::Bump;

use crate::{
    line::{write_lines_to, Line},
    patch::parsed_hunk::{HandleCheckError, ParsedHunk},
    write_to::WriteTo,
};

/// A group of lines starting with a "@@" line and not containing
/// other such lines; contains any number of changes
// Do not choose inner type at compile-time because we want to be able
// to replace individual hunks or diffs, i.e. without changing the
// outer type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hunk<'a> {
    UnParsed(&'a [Line<'a>]),
    Both(&'a [Line<'a>], ParsedHunk<'a>),
    Parsed(ParsedHunk<'a>),
}

/// How to initially parse the representation, which also has
/// implication on how it serializes back (use `Parsed`, not `Both`,
/// if you want serialization to always be regenerated from the parsed
/// representation)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseMode {
    /// Only retain the original lines where possible, parse them on
    /// demand where needed
    UnParsed,
    /// Parse immediately, but also retain the original lines where
    /// possible and use those for writing the serialization
    Both,
    /// Parse immediately and do not store the original lines, meaning
    /// serialization is always recreated from the parsed
    /// representation
    Parsed,
}

impl ParseMode {
    pub fn from_options(
        // Do a full parse (down to issues with changes, regardless of
        // `mode`) before splitting, implying additional checks like the
        // range checks unless those are disabled (see
        // `ignore_range_errors`).
        full_check: bool,
        // Regenerate the output from the fully parsed version; by
        // default, even with `full_check`, by default the original data
        // is re-used where possible. Indirectly implies `full_check` (as
        // it lazily parses everything anyway).
        regenerate: bool,
    ) -> Self {
        match (full_check, regenerate) {
            (false, false) => ParseMode::UnParsed,
            (true, false) => ParseMode::Both,
            (_, true) => ParseMode::Parsed,
        }
    }
}

impl<'a> WriteTo for Hunk<'a> {
    fn write_to(&self, out: impl Write) -> Result<(), std::io::Error> {
        match self {
            Hunk::UnParsed(lines) => write_lines_to(*lines, out),
            Hunk::Both(lines, _parsed_hunk) => write_lines_to(*lines, out),
            Hunk::Parsed(parsed_hunk) => parsed_hunk.write_to(out),
        }
    }
}

impl<'a> Hunk<'a> {
    /// If `parse` is true, parses the hunk contents, otherwise jut
    /// stores the original lines (the `parse` method will then parse
    /// it on demand)
    pub fn from_lines(
        lines: &'a [Line<'a>],
        bump: &'a Bump,
        parse_mode: ParseMode,
        handle_check_error: impl HandleCheckError,
    ) -> Result<Self> {
        match parse_mode {
            ParseMode::UnParsed => Ok(Hunk::UnParsed(lines)),
            ParseMode::Both => Ok(Hunk::Both(
                lines,
                ParsedHunk::from_lines(lines, bump, handle_check_error)?,
            )),
            ParseMode::Parsed => Ok(Hunk::Parsed(ParsedHunk::from_lines(
                lines,
                bump,
                handle_check_error,
            )?)),
        }
    }

    pub fn parsed<'b>(
        &self,
        bump: &'b Bump,
        handle_check_error: impl HandleCheckError,
    ) -> Result<Cow<'_, ParsedHunk<'b>>>
    where
        'a: 'b,
    {
        match self {
            Hunk::UnParsed(lines) => Ok(Cow::Owned(ParsedHunk::from_lines(
                lines,
                bump,
                handle_check_error,
            )?)),
            Hunk::Both(_line, parsed_hunk) => Ok(Cow::Borrowed(parsed_hunk)),
            Hunk::Parsed(parsed_hunk) => Ok(Cow::Borrowed(parsed_hunk)),
        }
    }
}

#[cfg(test)]
mod tests {
    use bstr::ByteSlice;
    use bumpalo::{
        collections::{self as bc, CollectIn},
        Bump,
    };

    use crate::patch::{
        change::Change,
        parsed_hunk::{CheckErrorClosure, MinimalHunkHead},
    };

    use super::*;

    fn l<'a>(line0: usize, s: &'a str) -> Line<'a> {
        Line::from_tuple((line0, s.as_ref()))
    }

    // Do consistency checks; split by changes already
    fn hunks_from_str<'b>(
        hunk_str: &'static str,
        bump: &'b Bump,
    ) -> Result<bc::Vec<'b, ParsedHunk<'b>>> {
        let lines: bc::Vec<_> = hunk_str
            .trim()
            .split("\n")
            .enumerate()
            .map(|(i, line)| Line::from_tuple((i, line.as_ref())))
            .collect_in(bump);
        let hunk = Hunk::from_lines(
            lines.into_bump_slice(),
            bump,
            ParseMode::Parsed,
            CheckErrorClosure(|e_| e_().map_err(Into::into)),
        )?;
        Ok(hunk
            .parsed(bump, CheckErrorClosure(|_| unreachable!()))?
            .split_by_change(bump))
    }

    #[test]
    fn t_split_hunk_into_changes() -> Result<()> {
        let mut bump = Bump::new();
        macro_rules! b {
            { $e:expr } => {
                &*bump.alloc($e)
            }
        }
        macro_rules! ba {
            { $($e:tt)* } => {
                &*bump.alloc([$($e)*])
            }
        }

        {
            // ,10 ,10 would be correct
            let wrong_range_str = r#"
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
"#;
            assert_eq!(
                hunks_from_str(wrong_range_str, &bump)
                    .err()
                    .unwrap()
                    .to_string(),
                "hunk range information on line 1 is inconsistent with body, expected:\n\
             @@ -550,10 +552,10 @@ fn cmp_function("
            );
        }
        bump.reset();

        {
            let correct_range_str = r#"
@@ -550,10 +552,10 @@ fn cmp_function(
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
"#;
            let hunks = hunks_from_str(correct_range_str, &bump)?;

            let change1 = b!(Change {
                post_from_previous_change: &[],
                pre: ba![
                    l(1, "}"),
                    l(2, ""),
                    l(3, "fn run_processing_commands<'t: 'u, 'u: 'v, 'v>("),
                ],
                minus: ba![l(4, "    items: &'v mut Vec<Item<'t>>,")],
                plus: ba![l(5, "    items: &'v mut Vec<Item<'t, &'t Path>>,")],
                post: ba![
                    l(6, "    cmds: &[ProcessingCommand],"),
                    l(7, "    now: SystemTime,"),
                    l(8, "    show_files_from_future: bool,"),
                ],
                backslash: false,
            });
            let change2 = b!(Change {
                post_from_previous_change: change1.post,
                pre: ba![],
                minus: ba![l(9, ") -> &'v [Item<'t>] {")],
                plus: ba![l(10, ") -> &'v [Item<'t, &'t Path>] {")],
                post: ba![
                    l(11, "    probe!(\"run_processing_commands\");"),
                    l(
                        12,
                        "    let mut selected_items = un_safe { hack_static(&mut **items) };",
                    ),
                ],
                backslash: false,
            });

            let expected_hunks = [
                ParsedHunk {
                    head: MinimalHunkHead {
                        orig_start: 550,
                        patched_start: 552,
                        head_post: Some(b"fn cmp_function(".as_bstr()),
                    },
                    changes: ba![change1],
                },
                ParsedHunk {
                    head: MinimalHunkHead {
                        orig_start: 554,
                        patched_start: 556,
                        head_post: Some(b"fn cmp_function(".as_bstr()),
                    },
                    changes: ba![change2],
                },
            ];
            assert_eq!(hunks, expected_hunks);

            // Check the lengths of the hunks (in original and patched
            // files)
            assert_eq!(
                expected_hunks.map(|h| h.full_hunk_head().to_minimal_hunk_head().1),
                [(7, 7), (6, 6),]
            );
        }
        bump.reset();

        {
            let long_middle_str = r#"@@ -381,14 +384,14 @@ impl EssentialMetadata {
 
 // Need PartialEq, Eq for tests
 #[derive(Debug, Clone, PartialEq, Eq)]
-pub struct Item<'region, P: PossiblySegmentedPath<'region>> {
+pub struct Item<'region, P: PossiblySegmentedPath<'region, INLINE>, INLINE> {
     pub path: P,
     pub metadata: EssentialMetadata,
     /// Metadata for the path if there was no error getting it
     pub link_target: Option<(Box<Path>, Option<Box<EssentialMetadata>>)>,
     // X X wanted to keep this field private to make it impossible to
     // create?
-    pub _phantom: PhantomData<&'region ()>,
+    pub _phantom: PhantomData<fn() -> &'region INLINE>,
 }
 
 #[test]
"#;
            let hunks = hunks_from_str(long_middle_str, &bump)?;

            let change1 = b!(Change {
                post_from_previous_change: &[],
                pre: ba![
                    l(1, ""),
                    l(2, "// Need PartialEq, Eq for tests"),
                    l(3, "#[derive(Debug, Clone, PartialEq, Eq)]"),
                ],
                minus: ba![l(
                    4,
                    "pub struct Item<'region, P: PossiblySegmentedPath<'region>> {"
                )],
                plus: ba![l(
                    5,
                    "pub struct Item<'region, P: PossiblySegmentedPath<'region, INLINE>, INLINE> {"
                )],
                post: ba![
                    l(6, "    pub path: P,"),
                    l(7, "    pub metadata: EssentialMetadata,"),
                    l(
                        8,
                        "    /// Metadata for the path if there was no error getting it"
                    ),
                    l(
                        9,
                        "    pub link_target: Option<(Box<Path>, Option<Box<EssentialMetadata>>)>,"
                    ),
                    l(
                        10,
                        "    // X X wanted to keep this field private to make it impossible to"
                    ),
                    l(11, "    // create?"),
                ],
                backslash: false,
            });
            let change2 = b!(Change {
                post_from_previous_change: change1.post,
                pre: ba![],
                minus: ba![l(12, "    pub _phantom: PhantomData<&'region ()>,")],
                plus: ba![l(
                    13,
                    "    pub _phantom: PhantomData<fn() -> &'region INLINE>,"
                )],
                post: ba![l(14, "}"), l(15, ""), l(16, "#[test]"),],
                backslash: false,
            });

            let expected_hunks = [
                ParsedHunk {
                    head: MinimalHunkHead {
                        orig_start: 381,
                        patched_start: 384,
                        head_post: Some(b"impl EssentialMetadata {".as_bstr()),
                    },
                    changes: ba![change1],
                },
                ParsedHunk {
                    head: MinimalHunkHead {
                        orig_start: 381 + 4,
                        patched_start: 384 + 4,
                        head_post: Some(b"impl EssentialMetadata {".as_bstr()),
                    },
                    changes: ba![change2],
                },
            ];
            assert_eq!(hunks, expected_hunks);
            assert_eq!(
                expected_hunks.map(|h| h.full_hunk_head().to_minimal_hunk_head().1),
                [(7, 7), (7, 7),]
            );
        }
        bump.reset();

        Ok(())
    }
}
