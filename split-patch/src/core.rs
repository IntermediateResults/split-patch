use std::{
    ffi::OsStr,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use bstr::{BStr, ByteSlice};
use bumpalo::Bump;
use cj_path_util::temp_file::temp_file_for;
use patchparser::{
    line::read_lines_in,
    make_bstring,
    patch::{
        diff::Diff,
        hunk::Hunk,
        parsed_hunk::{CheckError, HandleCheckError},
        patch::{Patch, PatchHead},
    },
    re,
    utils::add_suffix,
    write_to::WriteTo,
};

use crate::split_options::SplitOptions;
use anyhow::{Context, Result};

/// Receives the lines for a single diff. Returns the list of files created
fn split_diff_in<'a>(
    head: &PatchHead<'a>,
    // Guaranteed to be at least the "diff " line
    diff: &'a Diff<'a>,
    original_path: &Path,
    split_options: &SplitOptions,
    bump: &'a Bump,
) -> Result<Vec<Arc<Path>>> {
    let delete_index_line = true; // XX make configurable
    let b_path = diff.diff_path_b()?;

    let path = {
        let path_in_source_dir = add_suffix(
            original_path,
            OsStr::from_bytes(&make_bstring!({ b"-" } + { b_path.replace("/", b"_") })),
        )?;
        if let Some(output_dir) = &split_options.output_dir {
            output_dir.join(
                path_in_source_dir
                    .file_name()
                    .expect("expect file name to be present as suffix was added"),
            )
        } else {
            path_in_source_dir
        }
    };
    assert_ne!(*path, *original_path);

    let head_with_prefix =
        |prefix_part: &str| _head_with_prefix(split_options, b_path, head, prefix_part, bump);

    if split_options.mode.hunks() {
        // Old style sequence numbers, increasing monotonically for
        // all files, for when --changes is used with
        // --monotonous-numbers
        let mut file_i: usize = 0;
        let mut written_paths = Vec::new();

        macro_rules! diff_with_hunk {
            { $hunk:expr } => {
                diff.set_hunks(bump.alloc([$hunk]), delete_index_line, bump)
            }
        }

        if let Some(differences) = &diff.differences {
            for (hunk_i, hunk) in differences.hunks.iter().enumerate() {
                if split_options.mode.changes() {
                    for (change_i, change) in hunk
                        .parsed(bump, split_options)?
                        .split_by_change(bump)
                        .into_iter()
                        .enumerate()
                    {
                        let prefix_part = if split_options.monotonous_numbers {
                            format!("{file_i:03}")
                        } else {
                            format!("{hunk_i:03}-{change_i:03}")
                        };

                        let written_path = write_patch_file(
                            head_with_prefix(&prefix_part),
                            diff_with_hunk!(Hunk::Parsed(change)),
                            add_suffix(&path, format!("-{prefix_part}"))?,
                            split_options,
                        )?;

                        written_paths.push(written_path);
                        file_i += 1;
                    }
                } else {
                    let prefix_part = format!("{hunk_i:03}");

                    let written_path = write_patch_file(
                        head_with_prefix(&prefix_part),
                        diff_with_hunk!(hunk.clone()),
                        add_suffix(&path, format!("-{prefix_part}"))?,
                        split_options,
                    )?;

                    written_paths.push(written_path);
                }
            }
        } else {
            // Simply do not write split versions, OK? -- XX todo:
            // should write such files, at least for renames. (Perl
            // version doesn't, either.)
        }
        Ok(written_paths)
    } else {
        let written_path = write_patch_file(head_with_prefix(""), diff, path, split_options)?;

        Ok(vec![written_path])
    }
}

fn _head_with_prefix<'a>(
    split_options: &SplitOptions,
    b_path: &BStr,
    head: &'a PatchHead<'a>,
    prefix_part: &str,
    bump: &'a Bump,
) -> &'a PatchHead<'a> {
    if !split_options.subject_change {
        head
    } else {
        let prefix = if prefix_part.is_empty() {
            make_bstring!({ b_path } + { ": " })
        } else {
            make_bstring!({ b_path } + { " " } + { prefix_part } + { ": " })
        };
        head.update_header(
            "Subject",
            |value| {
                if split_options.insert_after_patch {
                    if let Some(cap) = re!(r"^(\s*\[PATCH\]\s*)(.*)").captures(value) {
                        return Some(make_bstring!({ &cap[1] } + { &prefix } + { &cap[2] }));
                    }
                }
                // Otherwise just simply:
                Some(make_bstring!({ &prefix } + { value }))
            },
            bump,
        )
        .0
    }
}

fn write_patch_file<'a>(
    head: &PatchHead<'a>,
    diff: &Diff<'a>,
    output_path: PathBuf,
    split_options: &SplitOptions,
) -> Result<Arc<Path>> {
    if split_options.dry_run {
        Ok(output_path.into())
    } else {
        let diffs = [diff];
        let patch = Patch {
            head,
            diffs: &diffs,
            footer: &[],
        };
        let mut file = temp_file_for(&*output_path, None)?;
        patch.write_to(&mut *file)?;
        Ok(file.persist()?)
    }
}

impl HandleCheckError for &SplitOptions {
    fn handle_check_error(&mut self, run_check: &dyn Fn() -> Result<(), CheckError>) -> Result<()> {
        if self.ignore_range_errors {
            Ok(())
        } else {
            run_check().map_err(Into::into)
        }
    }
}

/// Returns the list of files created
pub fn split_patch(patch_file_path: &Path, split_options: &SplitOptions) -> Result<Vec<Arc<Path>>> {
    let bump = Bump::new();
    let lines = read_lines_in(patch_file_path, &bump)?.into_bump_slice();
    let patch = Patch::from_lines(lines, &bump, split_options.parse_mode, split_options)?;

    let diffs = patch.diffs;

    // Write the diffs to individual (separate) files
    let mut written = Vec::new();
    for (diff_i, diff) in diffs.iter().enumerate() {
        let written_paths = split_diff_in(patch.head, diff, patch_file_path, split_options, &bump)
            .with_context(|| format!("splitting diff no. {}/{}", diff_i + 1, diffs.len()))?;

        written.extend(written_paths);
    }

    Ok(written)
}
