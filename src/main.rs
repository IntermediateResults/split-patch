pub mod re;
pub mod utils;

use std::{
    fs::read_to_string,
    io::{BufWriter, Write, stdout},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Ok, Result, anyhow, bail};
use cj_path_util::temp_file::temp_file_for;
use clap_with_warnings::clap_with_warnings;
use itertools::Itertools;
use regex::Captures;

use crate::utils::{add_suffix, take_while};

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
    diff: &str,
    original_path: &Path,
    split_options: &SplitOptions,
) -> Result<Vec<Arc<Path>>> {
    if !diff.starts_with("diff") {
        bail!("missing file in first line of diff: {:?}", diff);
    }

    let cap = re!(r"^diff.* (\S+)")
        .captures(diff)
        .context("missing file in the first line of diff")?;
    let prefix = {
        let file = &cap[1];

        file.strip_prefix("a/")
            .or_else(|| file.strip_prefix("b/"))
            .unwrap_or(file)
    };

    let path: Arc<Path> =
        add_suffix(original_path, &format!("-{}", prefix.replace("/", "_")))?.into();

    if *path == *original_path {
        bail!("path is the same as origpath: {}", path.display());
    }

    if split_options.hunks {
        let parsed_diff = parse_diff(diff)?;
        let hunks = if split_options.changes {
            parsed_diff
                .hunks
                .into_iter()
                .map(|hunk| split_hunk(&hunk))
                .collect::<Result<Vec<Vec<String>>>>()?
                .into_iter()
                .flatten()
                .collect()
        } else {
            parsed_diff.hunks
        };

        let mut written_paths = Vec::new();
        for (idx, hunk) in hunks.iter().enumerate() {
            let diff = [
                Some(parsed_diff.diff_line),
                parsed_diff.newfile_line,
                Some(parsed_diff.minus_line),
                Some(parsed_diff.plus_line),
                Some(hunk),
            ]
            .into_iter()
            .flatten()
            .join("\n");

            let suffix = format!("-{:03}", idx);

            let path2 = add_suffix(&path, &suffix)?;

            let mut file = temp_file_for(&*path2, None)?;

            let new_head = {
                let prefix = format!("{prefix} {suffix}: ");
                rewrite_head(head, &prefix, original_path)?
            };

            file.write_all(new_head.as_bytes())?;
            file.write_all(diff.as_bytes())?;

            // End the diff (file) with a newline
            if !diff.ends_with('\n') {
                eprintln!("doesnt end with newline");
                file.write_all(b"\n")?;
            }

            let written = file.persist()?;
            written_paths.push(written);
        }
        Ok(written_paths)
    } else {
        let new_head = {
            let prefix = format!("{}: ", prefix);
            rewrite_head(head, &prefix, original_path)?
        };

        let mut file = temp_file_for(&*path, None)?;
        (|| {
            file.write_all(new_head.as_bytes())?;
            file.write_all(diff.as_bytes())
        })()
        .with_context(|| anyhow!("writing to {:?}", file.temp_path()))?;
        file.persist()?;

        Ok(vec![path])
    }
}

fn rewrite_head(head: &[&str], prefix: &str, original_path: &Path) -> Result<String> {
    let re = re!(r"(?i)(\nsubject:\s*(?:\[PATCH]\s*)?)([^'n]*)");
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
            original_path.display()
        );
        Ok(head)
    }
}

struct ParsedDiff<'a> {
    diff_line: &'a str,
    newfile_line: Option<&'a str>,
    #[allow(unused)]
    index_line: Option<&'a str>,
    minus_line: &'a str,
    plus_line: &'a str,
    hunks: Vec<String>,
    // hunks: Vec<&'a str>,
}

fn parse_diff(diff: &str) -> Result<ParsedDiff<'_>> {
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
    let caps = re!(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? (.*)")
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
        // The group consists purely of lines starting with '-' and
        // '+' by its construction, hence:
        let group_plus_len = group.len() - group_minus_len;

        let orig_len = new_pre_len + group_minus_len + new_post_len;
        let patched_len = new_pre_len + group_plus_len + new_post_len;

        // XXX: Does the header need to be adapted to the following
        // patterns? As discovered for `split_hunk`
        // @@ -0,0 +1,2 @@
        // @@ -42 42 @@
        // @@ -42 +1,2 @@
        // @@ -0,0 +1 @@
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
            (|| {
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
