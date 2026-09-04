use std::io::{stdout, Write};

use anyhow::Result;
use bumpalo::Bump;
use patchparser::{
    patch::{
        change::Change,
        diff::{Diff, DiffDifferences},
        hunk::Hunk,
        parsed_hunk::{MinimalHunkHead, ParsedHunk},
        patch::{Patch, PatchHead, PatchHeadHeader},
    },
    write_to::WriteTo,
};

fn main() -> Result<()> {
    let hunk = ParsedHunk {
        head: MinimalHunkHead {
            orig_start: 1,
            patched_start: 4,
            head_post: Some("Head".as_ref()),
        },
        changes: &[
            &Change {
                post_from_previous_change: &[],
                pre: &["hello".into(), "world".into()],
                minus: &["foo".into(), "bar".into()],
                plus: &["food".into()],
                post: &["some".into(), "post".into(), "lines".into()],
                backslash: false,
            },
            &Change {
                post_from_previous_change: &[],
                pre: &["a new pre line".into()],
                minus: &["ome".into(), "thing".into()],
                plus: &["".into()],
                post: &["some".into(), "post".into(), "lines".into()],
                backslash: false,
            },
        ],
    };

    let mut out = stdout().lock();
    hunk.write_to(&mut out)?;

    writeln!(&mut out, "--------")?;
    let bump = &Bump::new();
    for (idx, hunk) in hunk.split_by_change(bump).iter().enumerate() {
        let diff_path_a = format!("a/foo-{idx}");
        let diff = Diff {
            diff_line: "diff -- blabla".into(),
            diff_path_a_full: Some(diff_path_a.as_str().as_ref()),
            diff_path_b_full: Some(
                bumpalo::format!(in bump, "b/foo-{idx}",)
                    .into_bump_str()
                    .as_ref(),
            ),
            newfile_line: None,
            deleted_line: None,
            similarity_line: None,
            rename_from_line: None,
            rename_to_line: None,
            differences: Some(DiffDifferences {
                index_line: Some("index line".into()),
                minus_line: "minus line".into(),
                plus_line: "plus line".into(),
                hunks: &[Hunk::Parsed(hunk.clone())],
            }),
        };

        // let patch = Patch::from_lines(lines, bump, parse_mode, handle_check_error)?;
        let patch = Patch {
            head: &PatchHead {
                header: Some(&PatchHeadHeader {
                    from_line: "From ...".into(),
                    header_lines: &["Author: bla".into()],
                }),
                remaining_lines: &["Hey there!".into(), "".into()],
            },
            diffs: &[&diff],
            footer: &[],
        };

        patch.write_to(&mut out)?;
        out.flush()?;
    }
    Ok(())
}
