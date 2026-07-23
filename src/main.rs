pub mod line;
pub mod patch;
pub mod re;
pub mod utils;

use std::{
    borrow::Cow,
    fs::read,
    io::{stdout, BufWriter, IoSlice, Write},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{anyhow, bail, Context, Result};
use bstr::ByteSlice;
use cj_path_util::temp_file::unbuffered_temp_file_for;
use clap_with_warnings::clap_with_warnings;
use format_bytes::format_bytes;
use regex::bytes::Captures;

use crate::{
    line::{write_lines_to, Line},
    patch::{diff::Diff, hunk::WriteAsHunk},
    utils::{add_suffix, split_before},
};

#[derive(Debug, clap::Args)]
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
#[command(version, about, long_about)]
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
fn write_diff(
    head_lines: &[Line],
    // Guaranteed to be at least the "diff " line
    diff_lines: &[Line],
    original_path: &Path,
    split_options: &SplitOptions,
) -> Result<Vec<Arc<Path>>> {
    let first_line = diff_lines[0];
    let cap = re!(r"^diff.* (\S+)")
        .captures(*first_line)
        .with_context(|| {
            format!(
                "missing 'diff' marker with file in the first line of the diff on line {first_line}",
            )
        })?;
    let prefix = {
        let file = &cap[1];

        file.strip_prefix(b"a/")
            .or_else(|| file.strip_prefix(b"b/"))
            .unwrap_or(file)
    };

    let path = {
        let path_in_old_dir = add_suffix(
            original_path,
            &format_bytes!(b"-{}", prefix.replace("/", b"_"))
                .to_str()
                .map_err(|e| anyhow!("{e}"))?,
        )?;
        if let Some(output_dir) = &split_options.output_dir {
            output_dir.join(
                path_in_old_dir
                    .file_name()
                    .expect("expect file name to be present as suffix was added"),
            )
        } else {
            path_in_old_dir
        }
    };

    assert_ne!(*path, *original_path);

    if split_options.hunks {
        let diff = Diff::from_lines(diff_lines.iter().copied())?;

        let diff_head = diff.head();

        // Old style sequence numbers, increasing monotonically for
        // all files, for when --changes is used with
        // --monotonous-numbers
        let mut file_i: usize = 0;

        let mut written_paths = Vec::new();
        for (hunk_i, hunk) in diff.hunks.iter().enumerate() {
            if split_options.changes {
                for (change_i, change) in hunk.split_into_changes()?.into_iter().enumerate() {
                    let mut diff_string: Vec<u8> = diff_head.clone().into();
                    change.write_as_hunk_to(&mut diff_string)?;
                    written_paths.push(write_patch_file(
                        head_with_subject_prefix(
                            split_options.no_subject_change,
                            head_lines,
                            if split_options.monotonous_numbers {
                                format_bytes!(b"{} {}: ", prefix, format!("{file_i:03}").as_bytes())
                            } else {
                                format_bytes!(
                                    b"{} {}-{}: ",
                                    prefix,
                                    format!("{hunk_i:03}").as_bytes(),
                                    format!("{change_i:03}").as_bytes()
                                )
                            },
                            original_path,
                        ),
                        &diff_string,
                        add_suffix(
                            &path,
                            &if split_options.monotonous_numbers {
                                format!("-{file_i:03}")
                            } else {
                                format!("-{hunk_i:03}-{change_i:03}")
                            },
                        )?
                        .into(),
                    )?);

                    file_i += 1;
                }
            } else {
                let mut diff_string: Vec<u8> = diff_head.clone().into();
                hunk.write_as_hunk_to(&mut diff_string)?;
                written_paths.push(write_patch_file(
                    head_with_subject_prefix(
                        split_options.no_subject_change,
                        head_lines,
                        format_bytes!(b"{} {}: ", prefix, format!("{hunk_i:03}").as_bytes()),
                        original_path,
                    ),
                    &diff_string,
                    add_suffix(&path, &format!("-{hunk_i:03}"))?.into(),
                )?);
            }
        }
        Ok(written_paths)
    } else {
        let mut diff_string: Vec<u8> = Vec::new();
        write_lines_to(head_lines, &mut diff_string)?;

        Ok(vec![write_patch_file(
            head_with_subject_prefix(
                split_options.no_subject_change,
                head_lines,
                format_bytes!(b"{}: ", prefix),
                original_path,
            ),
            &diff_string,
            path,
        )?])
    }
}

fn head_with_subject_prefix(
    no_subject_change: bool,
    head_lines: &[Line],
    prefix: Vec<u8>,
    original_path: &Path,
) -> Vec<u8> {
    let mut head: Vec<u8> = Vec::new();
    write_lines_to(head_lines, &mut head).expect("writing to Vec doesn't fail");
    if no_subject_change {
        return head;
    }
    let new_head = re!(r"(?i)(\nsubject:\s*(?:\[PATCH]\s*)?)([^'n]*)")
        .replace(&head, |c: &Captures| -> Vec<u8> {
            format_bytes!(b"{}{}{}", &c[1], prefix, &c[2])
        });
    match &new_head {
        Cow::Borrowed(_) => {
            eprintln!(
                "Warning: could not find subject line in file: {}",
                original_path.display()
            );
            head
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
fn split_patch(patch_file: &Path, split_options: &SplitOptions) -> Result<Vec<Arc<Path>>> {
    // 1. Read the patchfile
    let content = read(&patch_file)?;

    let content = re!(r"\n(?:-- \n(?:[^\n]*\n){0,3})?$").replace(&content, b"\n");

    let lines: Vec<Line> = content
        .split(|b| *b == b'\n')
        .enumerate()
        .map(Line::from_tuple)
        .collect();

    if lines.is_empty() {
        bail!("file has no lines"); // ?
    }

    // 2. Split the patches in the file to obtain the diffs

    let is_diff_line = |line: &Line| line.starts_with(b"diff ");

    let chunks = split_before(lines.iter().copied(), is_diff_line, |vec| vec);

    let (head, diffs): (&[Line], &[Vec<Line>]) =
        if chunks[0].first().map(is_diff_line).unwrap_or(false) {
            (&[], &chunks)
        } else {
            (&chunks[0], &chunks[1..])
        };
    if diffs.is_empty() {
        bail!("file does not appear to contain diffs");
    }

    // 3. Write the diffs to individual (separate) files
    let mut written = Vec::new();
    for diff in diffs {
        written.extend(write_diff(head, diff, &patch_file, split_options)?);
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
