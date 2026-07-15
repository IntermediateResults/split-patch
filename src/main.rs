use std::{
    env::{self, temp_dir},
    ffi::OsStr,
    fs::{self, File, read_to_string},
    io::{BufRead, BufReader, BufWriter, Read, Write, stdout},
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::{Path, PathBuf},
    str::Lines,
};

use anyhow::{Context, Ok, Result, bail};
use clap::Parser;
use regex::Regex;

/// Split the given patchfile(s) into new files
///
/// So that each new file only contains the part of the patch for
/// one particular target file.
#[derive(Debug, Parser)]
#[command(version, about, long_about)]
struct Args {
    /// Path to patch file
    // patch_file: Vec<PathBuf>,
    patch_file: PathBuf,

    /// Split on hunk boundaries, too.
    #[arg(long)]
    hunks: bool,

    /// Split on individual change groups, too (implies --hunks)
    #[arg(short, long)]
    changes: bool,

    /// Do not print the generated files.
    #[arg(short, long)]
    quiet: bool,
}

// XX: should the entire args really be passed?
// or create a sub args for what's truly needed here?
fn write_diff(head: &[&str], diff: &str, patch_filepath: &PathBuf, args: &Args) -> Result<()> {
    if !diff.starts_with("diff") {
        bail!("missing file in first line of diff: {:?}", diff);
    }

    let patch_filename = patch_filepath
        .file_name()
        .context("Failed to get filename of provided path to patchfile")?;

    // let patch_file_dir = patch_filepath.parent();
    let patch_file_dir = match patch_filepath.parent() {
        Some(parent) => parent,
        None => Path::new(""),
    };

    let re = Regex::new(r"^diff.* (\S+)")?;

    let file = re
        .captures(diff)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str())
        .context("missing file in the first line of diff")?;

    let prefix = file
        .strip_prefix("a/")
        .or_else(|| file.strip_prefix("b/"))
        .unwrap_or(file);

    let addon = prefix.replace("/", "_");
    let path = add_suffix(patch_filename, &addon)?;

    if *path == *patch_filename {
        bail!("path is the same as origpath: {path}");
    }

    if args.hunks {
        let parsed_diff = parse_diff(diff)?;
        if args.changes {
            let hunks = split_hunk(parsed_diff.hunks);
        }
        for hunk in hunks {}
    } else {
        let tmp_path_buf = temp_dir().join(&path);
        let tmp_path = Path::new(&tmp_path_buf);
        let new_head = rewrite_head(head, diff, patch_filename);

        let mut file = File::create(tmp_path).context("Failed to create temp file")?;
        file.write_all(new_head.as_bytes())?;

        file.metadata()?.permissions().set_mode(0666);
        fs::rename(tmp_path, patch_file_dir.join(path))?;

        if !args.quiet {
            let mut out = BufWriter::new(stdout());
            out.write_all(tmp_path.as_os_str().as_bytes())?;
            out.write_all(b"\n")?;
            // file.write_all(buf)
        }
    }

    Ok(())
}

fn rewrite_head(head: &[&str], diff: &str, patch_filename: &OsStr) -> String {
    todo!()
}

fn add_suffix(orig_path: &OsStr, addon: &str) -> Result<String> {
    let path = Path::new(&orig_path);
    match path.extension() {
        Some(ext) => {
            let stem = path
                .file_stem()
                .context("Failed to extract stem from patch filename")?
                .to_string_lossy();
            let parent = match path.parent() {
                Some(parent) => parent,
                None => Path::new(""),
            };

            Ok(parent
                .join(format!("{}-{}-{}", stem, addon, ext.to_string_lossy()))
                .to_string_lossy()
                .into_owned())
        }
        None => Ok(format!("{orig_path:?}-{addon}")),
    }
}

struct ParsedDiff<'a> {
    diff_line: &'a str,
    newfile_line: Option<&'a str>,
    index_line: Option<&'a str>,
    minus_line: &'a str,
    plus_line: &'a str,
    hunks: Vec<&'a str>,
}
fn parse_diff(diff: &str) -> Result<ParsedDiff> {
    // XXX: use slice instead
    let mut lines = diff.lines();

    let diff_line = lines
        .next()
        .filter(|l| l.starts_with("diff "))
        .context("invalid patch file format [missing diff line]: {diff}")?;

    let mut line = lines
        .next()
        .context("invalid patch file format [diff ended unexpectedly]: {diff}")?;

    let newfile_line = if line.starts_with("new file mode ") {
        let l = line;
        line = lines
            .next()
            .context("invalid patch file format [missing `index` or `---` line]: {diff}")?;
        Some(l)
    } else {
        None
    };

    let index_line = if line.starts_with("index ") {
        let l = line;
        line = lines
            .next()
            .context("invalid patch file format [missing `---` line]: {diff}")?;
        Some(l)
    } else {
        None
    };

    if !line.starts_with("--- ") {
        bail!("invalid patch file format [expected `---` line]: {diff}");
    }
    let minus_line = line;

    line = lines
        .next()
        .context("invalid patch file format [missing `+++` line]: {diff}")?;
    if !line.starts_with("+++ ") {
        bail!("invalid patch file format [expected `+++` line]: {diff}");
    }
    let plus_line = line;

    // rest: everything after `+++` line
    let plus_start = diff.find(plus_line).context("Should have found the `+++` line")?;
    let rest_start = plus_start + plus_line.len() + 1;
    let rest = &diff[rest_start..];

    let hunks = gather_hunks(&rest);

    Ok(ParsedDiff {
        diff_line,
        newfile_line,
        index_line,
        minus_line,
        plus_line,
        hunks,
    })
}

fn gather_hunks(s: &str) -> Vec<&str> {
    let mut hunks = Vec::new();
    let mut start = 0;

    for (idx, _) in s.match_indices("\n@@ ") {
        let split_at = idx + 1; // newline stays with previous hunk
        hunks.push(&s[start..split_at]);
        start = split_at
    }

    hunks.push(&s[start..]);
    hunks
}

fn split_hunk(hunks: Vec<&'a str>) -> Result<()> {
    todo!()
}
fn main() -> Result<()> {
    let mut args = Args::parse();

    if args.changes {
        args.hunks = true;
    }

    // 1. Read the patchfile in the current directory
    let content = read_to_string(&args.patch_file)?;

    let re = Regex::new(r"\n(?:-- \n(?:[^\n]*\n){0,3})?$")?;
    let content = re.replace(&content, "\n");

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
        bail!(
            "file does not appear to contain diffs: {:#?}",
            &args.patch_file
        );
    };

    for diff in diffs {
        let result = write_diff(head, diff, &args.patch_file, &args)?;
    }
    // dbg!((&head).len());
    // dbg!(&head);
    // dbg!((&diffs).len());
    // dbg!(&diffs);

    // 3. Write the diffs to individual (separate) files

    Ok(())
}
