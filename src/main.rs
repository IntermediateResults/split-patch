use std::{
    env::temp_dir,
    ffi::OsStr,
    fs::{self, File, read_to_string},
    io::{BufWriter, Write, stdout},
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::{Path, PathBuf},
};

use anyhow::{Context, Ok, Result, bail};
use clap::Parser;
use regex::{Captures, Regex};
use tempfile::NamedTempFile;

/// Split the given patchfile(s) into new files
///
/// So that each new file only contains the part of the patch for
/// one particular target file.
#[derive(Debug, Parser)]
#[command(version, about, long_about)]
struct Args {
    /// Path to patch file
    #[arg(required = true)]
    patch_file: Vec<PathBuf>,

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
fn write_diff(head: &[&str], diff: &str, patch_filepath: &Path, args: &Args) -> Result<()> {
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

    let prefix = {
        let file = re
            .captures(diff)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str())
            .context("missing file in the first line of diff")?;

        file.strip_prefix("a/")
            .or_else(|| file.strip_prefix("b/"))
            .unwrap_or(file)
    };

    let addon = prefix.replace("/", "_");
    let path = add_suffix(patch_filename, &addon)?;

    if *path == *patch_filename {
        bail!("path is the same as origpath: {path}");
    }

    if args.hunks {
        let mut parsed_diff = parse_diff(diff)?;
        if args.changes {
            parsed_diff.hunks = parsed_diff
                .hunks
                .into_iter()
                .map(|hunk| split_hunk(&hunk))
                .collect::<Result<Vec<Vec<String>>>>()?
                .into_iter()
                .flatten()
                .collect();
        }
        for (idx, hunk) in parsed_diff.hunks.iter().enumerate() {
            let diff = [
                Some(parsed_diff.diff_line),
                parsed_diff.newfile_line,
                Some(parsed_diff.minus_line),
                Some(parsed_diff.plus_line),
                Some(hunk),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("\n");

            let suffix = format!("{:03}", idx);

            let path2 = add_suffix(OsStr::new(&path), &suffix)?;

            let mut file = NamedTempFile::new()?;

            let new_head = {
                let prefix = format!("{prefix} {suffix}: ");
                rewrite_head(head, &prefix, patch_filename)
            }?;

            file.write_all(new_head.as_bytes())?;
            file.write_all(diff.as_bytes())?;

            // End the diff (file) with a newline
            if !diff.ends_with('\n') {
                println!("doesnt end with newline");
                file.write_all(b"\n")?;
            }
            file.path().metadata()?.permissions().set_mode(0o666);

            // XXX: move this out of if/else block to prevent duplication
            if !args.quiet {
                let mut out = BufWriter::new(stdout().lock());
                out.write_all(patch_file_dir.join(&path2).as_os_str().as_bytes())?;
                out.write_all(b"\n")?;
                // file.write_all(buf)
            }

            file.persist(patch_file_dir.join(path2))?;
            // XXX: will other processes access the file?
            // file.into_temp_path();
        }
    } else {
        let tmp_path_buf = temp_dir().join(&path);
        let tmp_path = Path::new(&tmp_path_buf);
        let new_head = {
            let prefix = format!("{}: ", prefix);
            rewrite_head(head, &prefix, patch_filename)
        }?;

        let mut file = File::create(tmp_path).context("Failed to create temp file")?;
        file.write_all(new_head.as_bytes())?;
        file.write_all(diff.as_bytes())?;

        file.metadata()?.permissions().set_mode(0o666);
        fs::rename(tmp_path, patch_file_dir.join(path))?;

        if !args.quiet {
            let mut out = BufWriter::new(stdout().lock());
            out.write_all(tmp_path.as_os_str().as_bytes())?;
            out.write_all(b"\n")?;
            // file.write_all(buf)
        }
    }

    Ok(())
}

fn rewrite_head(head: &[&str], prefix: &str, patch_filename: &OsStr) -> Result<String> {
    let re = Regex::new(r"(?i)(\nsubject:\s*(?:\[PATCH]\s*)?)([^'n]*)")?;
    let head = head.join("\n");
    if re.is_match(&head) {
        Ok(re
            .replace(&head, |caps: &Captures| {
                format!("{}{}{}", &caps[1], prefix, &caps[2])
            })
            .into_owned())
    } else {
        eprintln!(
            "Warning: could not find subject in head of file: {}",
            patch_filename.display()
        );
        Ok(head)
    }
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
                .join(format!("{}-{}.{}", stem, addon, ext.to_string_lossy()))
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
    hunks: Vec<String>,
    // hunks: Vec<&'a str>,
}
fn parse_diff(diff: &str) -> Result<ParsedDiff> {
    // XXX: use slice instead
    let mut lines = diff.lines();

    let diff_line = lines
        .next()
        .filter(|l| l.starts_with("diff "))
        .context(format!(
            "invalid patch file format [missing diff line]: {diff}"
        ))?;

    let mut line = lines.next().context(format!(
        "invalid patch file format [diff ended unexpectedly]: {diff}"
    ))?;

    let newfile_line = if line.starts_with("new file mode ") {
        let l = line;
        line = lines.next().context(format!(
            "invalid patch file format [missing `index` or `---` line]: {diff}"
        ))?;
        Some(l)
    } else {
        None
    };

    let index_line = if line.starts_with("index ") {
        let l = line;
        line = lines.next().context(format!(
            "invalid patch file format [missing `---` line]: {diff}"
        ))?;
        Some(l)
    } else {
        None
    };

    if !line.starts_with("--- ") {
        bail!("invalid patch file format [expected `---` line]: {diff}");
    }
    let minus_line = line;

    line = lines.next().context(format!(
        "invalid patch file format [missing `+++` line]: {diff}"
    ))?;
    if !line.starts_with("+++ ") {
        bail!("invalid patch file format [expected `+++` line]: {diff}");
    }
    let plus_line = line;

    // rest: everything after `+++` line
    let plus_start = diff
        .find(plus_line)
        .context("Should have found the `+++` line")?;
    let rest_start = plus_start + plus_line.len() + 1;
    let rest = &diff[rest_start..];

    let hunks = gather_hunks(rest).into_iter().map(str::to_owned).collect();

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

fn split_hunk<'a>(hunks: &'a str) -> Result<Vec<String>> {
    let lines: Vec<&'a str> = hunks.lines().collect();

    let head = lines.first().context("hunk is empty")?;

    // XXX: are all these valid patterns? What happens if captures
    // fail?
    // @@ -0,0 +1,2 @@
    // @@ -42 42 @@
    // @@ -42 +1,2 @@
    // @@ -0,0 +1 @@
    let re = Regex::new(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? (.*)")?;

    let caps = re
        .captures(head)
        .context(format!("invalid hunk head: {head}"))?;

    let mut orig_start: usize = caps[1].parse()?;
    let mut patched_start: usize = caps[3].parse()?;
    let head_post = &caps[5];

    let mut remaining: &[&str] = &lines[1..];
    let mut result = Vec::new();

    while !remaining.is_empty() {
        let (pre, rest) = take_while(remaining, |l| l.starts_with(' '));
        let (group, rest2) = take_while(rest, |l| l.starts_with(['-', '+']));
        let (post, rest3) = take_while(rest2, |l| l.starts_with(' '));

        let pre_len = pre.len();

        let new_pre = if pre.len() > 3 {
            &pre[pre.len() - 3..]
        } else {
            pre
        };
        let new_pre_len = new_pre.len();

        let new_post = if post.len() > 3 { &post[..3] } else { post };
        let new_post_len = new_post.len();

        let group_minus_len = group.iter().filter(|l| l.starts_with('-')).count();
        // XXX is it safe to assume that ?
        // group_minus_len = group.len() - group_minus_len
        let group_plus_len = group.iter().filter(|l| l.starts_with('+')).count();

        let orig_len = new_pre_len + group_minus_len + new_post_len;
        let patched_len = new_pre_len + group_plus_len + new_post_len;

        let header = format!(
            "@@ -{},{} +{},{} {}",
            orig_start, orig_len, patched_start, patched_len, head_post
        );

        let mut out = vec![header];
        out.extend(new_pre.iter().map(|s| s.to_string()));
        out.extend(group.iter().map(|s| s.to_string()));
        out.extend(new_post.iter().map(|s| s.to_string()));

        result.push(out.join("\n"));

        if rest3.is_empty() {
            break;
        }

        orig_start += pre_len + group_minus_len;
        patched_start += pre_len + group_plus_len;

        remaining = rest2;
    }

    Ok(result)
}
fn take_while<'a>(
    lines: &'a [&'a str],
    predicate: impl Fn(&str) -> bool,
) -> (&'a [&'a str], &'a [&'a str]) {
    let count = lines.iter().take_while(|l| predicate(l)).count();

    lines.split_at(count)
}
fn main() -> Result<()> {
    let mut args = Args::parse();

    // `--changes` implies `--hunks`
    // XXX: perhaps this can be handled natively by `clap`
    if args.changes {
        args.hunks = true;
    }

    for file in &args.patch_file {
        // 1. Read the patchfile
        let content = read_to_string(&file)?;

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
            bail!("file does not appear to contain diffs: {:#?}", &file);
        };

        // 3. Write the diffs to individual (separate) files
        for diff in diffs {
            write_diff(head, diff, &file, &args)?;
        }
    }

    Ok(())
}
