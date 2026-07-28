use std::{
    ffi::OsStr,
    io::{stdout, BufWriter, Write},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{anyhow, Context, Result};
use bstr::{BStr, ByteSlice};
use bumpalo::Bump;
use cj_path_util::temp_file::temp_file_for;
use clap_with_warnings::clap_with_warnings;
use split_patch::{
    make_bstring,
    patch::{
        diff::Diff,
        patch::{OwnedPatch, PatchHead},
    },
    re,
    utils::add_suffix,
    write_to::WriteTo,
};

#[derive(Debug, clap::Args)]
#[command(allow_hyphen_values = true)]
struct SplitOptions {
    /// Split on hunk boundaries, not just file boundaries.
    #[clap(long)]
    hunks: bool,

    /// Split on individual change groups, too (implies `--hunks`)
    #[clap(short, long)]
    changes: bool,

    /// Omit the addition of a prefix to the subject line of patch
    /// files that have a git style patch header
    #[clap(long)]
    no_subject_change: bool,

    /// When using `--changes`, use a single number counter for
    /// generating the ids for the generated output file names and
    /// subject prefixes instead of `{hunk_id}-{change_id}`.
    #[clap(long)]
    monotonous_numbers: bool,

    /// Path to the directory where to write the split files
    /// to. Default: the same directory as the input file.
    #[clap(long)]
    output_dir: Option<PathBuf>,

    /// Do not insert the prefix after "[PATCH]", but before
    /// everything.
    #[clap(long)]
    no_insert_after_patch: bool,
}

/// Split the given patchfile(s) into new files
///
/// So that each new file only contains the part of the patch for
/// one particular target file.
#[clap_with_warnings]
#[derive(Debug, clap::Parser)]
#[command(version, about, long_about, allow_hyphen_values = true)]
struct Args {
    /// Path(s) to patch file(s)
    #[clap(required = true)]
    patch_file: Vec<PathBuf>,

    #[clap(flatten)]
    split_options: SplitOptions,

    /// Do not print the list of generated files.
    #[clap(short, long)]
    quiet: bool,
}

/// Receives the lines for a single diff. Returns the list of files created
fn split_diff<'head, 'head_a, 'diff, 'diff_a>(
    head: &'head PatchHead<'head_a>,
    // Guaranteed to be at least the "diff " line
    diff: &'diff Diff<'diff_a>,
    original_path: &Path,
    split_options: &SplitOptions,
) -> Result<Vec<Arc<Path>>>
where
    'head_a: 'head,
    'diff_a: 'diff,
{
    let delete_index_line = true; // XX make configurable
    let b_path = diff.diff_path_b()?;

    let path = {
        let path_in_source_dir = add_suffix(
            original_path,
            OsStr::from_bytes(&*make_bstring!({ b"-" } + { b_path.replace("/", b"_") })),
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

    let bump = Bump::new();
    let head_with_prefix =
        |prefix_part: &str| _head_with_prefix(split_options, b_path, head, prefix_part, &bump);

    if split_options.hunks {
        // Old style sequence numbers, increasing monotonically for
        // all files, for when --changes is used with
        // --monotonous-numbers
        let mut file_i: usize = 0;
        let mut written_paths = Vec::new();

        macro_rules! diff_with_hunk {
            { $hunk:expr } => {
                diff.reborrow()
                    .set_hunks(vec![$hunk], delete_index_line)
            }
        }

        if let Some(differences) = &diff.differences {
            for (hunk_i, hunk) in differences.hunks.iter().enumerate() {
                if split_options.changes {
                    for (change_i, change) in hunk.split_into_changes()?.into_iter().enumerate() {
                        let prefix_part = if split_options.monotonous_numbers {
                            format!("{file_i:03}")
                        } else {
                            format!("{hunk_i:03}-{change_i:03}")
                        };

                        let written_path = write_patch_file(
                            &head_with_prefix(&prefix_part),
                            diff_with_hunk!(change.to_hunk(&bump)),
                            add_suffix(&path, format!("-{prefix_part}"))?.into(),
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
        let written_path = write_patch_file(head_with_prefix(""), diff, path)?;

        Ok(vec![written_path])
    }
}

fn _head_with_prefix<'a: 'b, 'b>(
    split_options: &SplitOptions,
    b_path: &BStr,
    head: &PatchHead<'a>,
    prefix_part: &str,
    bump: &'b Bump,
) -> &'b PatchHead<'b> {
    if split_options.no_subject_change {
        bump.alloc(head.reborrow())
    } else {
        let mut head = head.reborrow();
        let prefix = if prefix_part.is_empty() {
            make_bstring!({ b_path } + { ": " })
        } else {
            make_bstring!({ b_path } + { " " } + { prefix_part } + { ": " })
        };
        head.update_header(
            "Subject",
            |value| {
                if !split_options.no_insert_after_patch {
                    if let Some(cap) = re!(r"^(\s*\[PATCH\]\s*)(.*)").captures(value) {
                        return Some(make_bstring!({ &cap[1] } + { &prefix } + { &cap[2] }));
                    }
                }
                // Otherwise just simply:
                Some(make_bstring!({ &prefix } + { value }))
            },
            bump,
        );
        bump.alloc(head)
    }
}

fn write_patch_file<'t1, 't2, 't3, 't4>(
    head: &'t1 PatchHead<'t2>,
    diff: &'t3 Diff<'t4>,
    output_path: PathBuf,
) -> Result<Arc<Path>> {
    let mut file = temp_file_for(&*output_path, None)?;
    head.write_to(&mut *file)?;
    diff.write_to(&mut *file)?;
    Ok(file.persist()?)
}

/// Returns the list of files created
fn split_patch(patch_file_path: &Path, split_options: &SplitOptions) -> Result<Vec<Arc<Path>>> {
    let owned_patch = OwnedPatch::from_path(patch_file_path)?;
    let patch = owned_patch.parsed()?;
    let diffs = &*patch.diffs;

    // Write the diffs to individual (separate) files
    let mut written = Vec::new();
    for (diff_i, diff) in diffs.iter().enumerate() {
        let written_paths = split_diff(&patch.head, &diff, patch_file_path, split_options)
            .with_context(|| format!("splitting diff no. {}/{}", diff_i + 1, diffs.len()))?;

        written.extend(written_paths);
    }

    Ok(written)
}

fn main() -> Result<()> {
    let mut args = Args::parse();

    // `--changes` implies `--hunks`
    // XXX: perhaps this can be handled natively by `clap`
    if args.split_options.changes {
        args.split_options.hunks = true;
    }

    for patch_file in &args.patch_file {
        let written = split_patch(&patch_file, &args.split_options)
            .with_context(|| anyhow!("splitting the patch file {patch_file:?}"))?;

        if !args.quiet {
            (|| -> Result<_> {
                let mut out = BufWriter::new(stdout().lock());
                for path in written {
                    out.write_all(path.as_os_str().as_bytes())?;
                    out.write_all(b"\n")?;
                }
                Ok(())
            })()
            .context("writing to stdout")?
        }
    }

    Ok(())
}
