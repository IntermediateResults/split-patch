use std::{
    borrow::Cow,
    ffi::OsStr,
    io::{stdout, BufWriter, IoSlice, Write},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{anyhow, bail, Context, Result};
use bstr::{BString, ByteSlice};
use cj_path_util::temp_file::unbuffered_temp_file_for;
use clap_with_warnings::clap_with_warnings;
use regex::bytes::Captures;

use split_patch::{
    make_bstring,
    patch::{
        diff::Diff,
        hunk::WriteAsHunk,
        patch::{PatchFile, PatchHead},
    },
    re,
    utils::add_suffix,
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
fn split_diff(
    head: &PatchHead<'_>,
    // Guaranteed to be at least the "diff " line
    diff: &Diff,
    original_path: &Path,
    split_options: &SplitOptions,
) -> Result<Vec<Arc<Path>>> {
    let prefix = diff.diff_path_b()?;

    let path = {
        let path_in_source_dir = add_suffix(
            original_path,
            OsStr::from_bytes(&*make_bstring!({ b"-" } + { prefix.replace("/", b"_") })),
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

    if split_options.hunks {
        let diff_head = diff.head_to_string(false);

        // Old style sequence numbers, increasing monotonically for
        // all files, for when --changes is used with
        // --monotonous-numbers
        let mut file_i: usize = 0;
        let mut written_paths = Vec::new();

        if let Some(differences) = &diff.differences {
            for (hunk_i, hunk) in differences.hunks.iter().enumerate() {
                if split_options.changes {
                    for (change_i, change) in hunk.split_into_changes()?.into_iter().enumerate() {
                        let mut diff_string: Vec<u8> = diff_head.clone().into();
                        change.write_as_hunk_to(&mut diff_string)?;

                        let written_path = write_patch_file(
                            head_with_subject_prefix(
                                split_options.no_subject_change,
                                head,
                                if split_options.monotonous_numbers {
                                    make_bstring!({ prefix } + (" {file_i:03}: "))
                                } else {
                                    make_bstring!({ prefix } + (" {hunk_i:03}-{change_i:03}: "))
                                },
                                original_path,
                            ),
                            &diff_string,
                            add_suffix(
                                &path,
                                if split_options.monotonous_numbers {
                                    format!("-{file_i:03}")
                                } else {
                                    format!("-{hunk_i:03}-{change_i:03}")
                                }
                                .as_ref(),
                            )?
                            .into(),
                        )?;

                        written_paths.push(written_path);
                        file_i += 1;
                    }
                } else {
                    let mut diff_string: Vec<u8> = diff_head.clone().into();
                    hunk.write_as_hunk_to(&mut diff_string)?;

                    let written_path = write_patch_file(
                        head_with_subject_prefix(
                            split_options.no_subject_change,
                            head,
                            make_bstring!({ prefix } + (" {hunk_i:03}: ")),
                            original_path,
                        ),
                        &diff_string,
                        add_suffix(&path, format!("-{hunk_i:03}").as_ref())?,
                    )?;

                    written_paths.push(written_path);
                }
            }
        } else {
            // Simply do not write split versions, OK?
        }
        Ok(written_paths)
    } else {
        let mut diff_string: Vec<u8> = Vec::new();
        diff.write_to(&mut diff_string)?;

        let written_path = write_patch_file(
            head_with_subject_prefix(
                split_options.no_subject_change,
                head,
                make_bstring!({ prefix } + { ": " }),
                original_path,
            ),
            &diff_string,
            path,
        )?;

        Ok(vec![written_path])
    }
}

fn head_with_subject_prefix(
    no_subject_change: bool,
    head: &PatchHead<'_>,
    prefix: BString,
    original_path: &Path,
) -> Vec<u8> {
    let mut head_string: Vec<u8> = Vec::new();
    head.write_to(&mut head_string)
        .expect("writing to Vec doesn't fail");
    if no_subject_change {
        return head_string;
    }
    let new_head = re!(r"(?i)(\nsubject:\s*(?:\[PATCH]\s*)?)([^'n]*)")
        .replace(&head_string, |c: &Captures| -> BString {
            make_bstring!({ &c[1] } + { &*prefix } + { &c[2] })
        });

    // `regex` crate's way to allow to warn about not matching; also
    // optimize by avoiding a copy of the borrow in that case
    match &new_head {
        Cow::Borrowed(_) => {
            eprintln!(
                "Warning: could not find subject line in file: {}",
                original_path.display()
            );
            head_string
        }
        Cow::Owned(_) => new_head.into_owned(),
    }
}

fn write_patch_file(new_head: Vec<u8>, diff: &[u8], output_path: PathBuf) -> Result<Arc<Path>> {
    let expected_n_written = new_head.len() + diff.len();
    let mut file = unbuffered_temp_file_for(&*output_path, None)?;
    let n_written = file
        .write_vectored(&[IoSlice::new(&new_head), IoSlice::new(diff)])
        .with_context(|| anyhow!("writing to {:?}", file.temp_path()))?;
    if n_written != expected_n_written {
        bail!("could only write {n_written} out of {expected_n_written} bytes to {output_path:?}");
    }
    Ok(file.persist()?)
}

/// Returns the list of files created
fn split_patch(patch_file_path: &Path, split_options: &SplitOptions) -> Result<Vec<Arc<Path>>> {
    let patch_file = PatchFile::from_path(patch_file_path)?;
    let patch = patch_file.patch();
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
