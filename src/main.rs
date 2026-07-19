pub mod patch;
pub mod re;
pub mod utils;

use std::{
    borrow::Cow,
    fs::read_to_string,
    io::{BufWriter, IoSlice, Write, stdout},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow, bail};
use cj_path_util::temp_file::unbuffered_temp_file_for;
use clap_with_warnings::clap_with_warnings;
use regex::Captures;

use crate::{
    patch::{diff::Diff, hunk::WriteAsHunk},
    utils::add_suffix,
};

#[derive(Debug, clap::Args)]
struct SplitOptions {
    /// Split on hunk boundaries, too.
    #[clap(long)]
    hunks: bool,

    /// Split on individual change groups, too (implies --hunks)
    #[clap(short, long)]
    changes: bool,
}

/// Split the given patchfile(s) into new files
///
/// So that each new file only contains the part of the patch for
/// one particular target file.
#[clap_with_warnings]
#[derive(Debug, clap::Parser)]
#[command(version, about, long_about)]
struct Args {
    /// Path to patch file(s)
    #[clap(required = true)]
    patch_file: Vec<PathBuf>,

    #[clap(flatten)]
    split_options: SplitOptions,

    /// Do not print the generated files.
    #[clap(short, long)]
    quiet: bool,
}

/// Returns the list of files created
fn write_diff(
    head: &[&str],
    diff_str: &str,
    original_path: &Path,
    split_options: &SplitOptions,
) -> Result<Vec<Arc<Path>>> {
    let cap = re!(r"^diff.* (\S+)")
        .captures(diff_str)
        .context("missing 'diff' marker with file in the first line of the diff")?;
    let prefix = {
        let file = &cap[1];

        file.strip_prefix("a/")
            .or_else(|| file.strip_prefix("b/"))
            .unwrap_or(file)
    };

    let path = add_suffix(original_path, &format!("-{}", prefix.replace("/", "_")))?;

    assert_ne!(*path, *original_path);

    if split_options.hunks {
        let diff = Diff::from_str(diff_str)?;

        let diff_head = diff.head();

        let mut written_paths = Vec::new();
        for (hunk_i, hunk) in diff.hunks.iter().enumerate() {
            if split_options.changes {
                for (change_i, change) in hunk.split_into_changes()?.into_iter().enumerate() {
                    let mut diff_string: Vec<u8> = diff_head.clone().into();
                    change.write_as_hunk_to(&mut diff_string)?;
                    written_paths.push(write_patch_file(
                        head_with_subject_prefix(
                            head,
                            format!("{prefix} {hunk_i:03}-{change_i:03}: "),
                            original_path,
                        ),
                        &diff_string,
                        add_suffix(&path, &format!("-{hunk_i:03}-{change_i:03}"))?.into(),
                    )?);
                }
            } else {
                let mut diff_string: Vec<u8> = diff_head.clone().into();
                hunk.write_as_hunk_to(&mut diff_string)?;
                written_paths.push(write_patch_file(
                    head_with_subject_prefix(
                        head,
                        format!("{prefix} {hunk_i:03}: "),
                        original_path,
                    ),
                    &diff_string,
                    add_suffix(&path, &format!("-{hunk_i:03}"))?.into(),
                )?);
            }
        }
        Ok(written_paths)
    } else {
        Ok(vec![write_patch_file(
            head_with_subject_prefix(head, format!("{}: ", prefix), original_path),
            diff_str.as_bytes(),
            path,
        )?])
    }
}

fn head_with_subject_prefix(head: &[&str], prefix: String, original_path: &Path) -> String {
    let head = head.join("\n");
    let new_head = re!(r"(?i)(\nsubject:\s*(?:\[PATCH]\s*)?)([^'n]*)")
        .replace(&head, |c: &Captures| {
            format!("{}{}{}", &c[1], prefix, &c[2])
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

fn write_patch_file(new_head: String, diff: &[u8], output_path: PathBuf) -> Result<Arc<Path>> {
    let new_head = new_head.as_bytes();
    let expected_n_written = new_head.len() + diff.len();
    let mut file = unbuffered_temp_file_for(&*output_path, None)?;
    let n_written = file
        .write_vectored(&[IoSlice::new(new_head), IoSlice::new(diff)])
        .with_context(|| anyhow!("writing to {:?}", file.temp_path()))?;
    if n_written != expected_n_written {
        bail!("could only write {n_written} out of {expected_n_written} bytes to {output_path:?}");
    }
    Ok(file.persist()?)
}

/// Returns the list of files created
fn split_patch(patch_file: &Path, split_options: &SplitOptions) -> Result<Vec<Arc<Path>>> {
    // 1. Read the patchfile
    let content = read_to_string(&patch_file)?;

    let content = re!(r"\n(?:-- \n(?:[^\n]*\n){0,3})?$").replace(&content, "\n");

    // 2. Split the patches in the file to obtain the diffs
    let mut chunks = Vec::new();
    let mut start = 0;
    for (idx, _) in content.match_indices("\ndiff ") {
        let split_at = idx + 1;
        chunks.push(&content[start..split_at]);
        start = split_at;
    }

    chunks.push(&content[start..]);

    let Some((head, diffs)) = chunks.split_at_checked(1) else {
        bail!("file does not appear to contain diffs");
    };

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
