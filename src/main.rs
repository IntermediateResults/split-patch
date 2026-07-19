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
use itertools::Itertools;
use regex::Captures;

use crate::{
    re::GetStr,
    utils::{add_suffix, split_before, take_while},
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
        let diff = parse_diff(diff_str)?;

        let diff_head = diff.head();

        let mut written_paths = Vec::new();
        for (hunk_i, hunk) in diff.hunks.iter().enumerate() {
            if split_options.changes {
                for (change_i, change) in split_hunk_into_changes(hunk)?.into_iter().enumerate() {
                    let mut diff_string: Vec<u8> = diff_head.clone().into();
                    change.write_to(&mut diff_string)?;
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
                hunk.write_to(&mut diff_string)?;
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

/// A group of lines starting with a "@@" line and not containing
/// other such lines; contains any number of changes
struct Hunk<'a> {
    /// The Vec is never empty, at least the "@@ " line is ensured by
    /// construction via `split_before` which does not create a group
    /// out of no lines.
    lines: Vec<(usize, &'a str)>,
}

impl<'a> Hunk<'a> {
    fn write_to(&self, mut out: impl Write) -> Result<(), std::io::Error> {
        for (_, line) in &self.lines {
            writeln!(&mut out, "{line}")?;
        }
        Ok(())
    }
}

/// A single group of "-" and "+" lines and context around them; a
/// number of changes make up a hunk
struct Change<'a, 'h> {
    orig_start: usize,
    orig_len: usize,
    patched_start: usize,
    patched_len: usize,
    head_post: &'a str,
    pre: &'h [(usize, &'a str)],
    group: &'h [(usize, &'a str)],
    post: &'h [(usize, &'a str)],
}

impl<'a, 'h> Change<'a, 'h> {
    fn write_to(&self, mut out: impl Write) -> Result<(), std::io::Error> {
        let Self {
            orig_start,
            orig_len,
            patched_start,
            patched_len,
            head_post,
            pre,
            group,
            post,
        } = self;
        // XXX: Does the header need to be adapted to the following
        // patterns? As discovered for `split_hunk`
        // @@ -0,0 +1,2 @@
        // @@ -42 42 @@
        // @@ -42 +1,2 @@
        // @@ -0,0 +1 @@
        writeln!(
            &mut out,
            "@@ -{},{} +{},{} {}",
            orig_start, orig_len, patched_start, patched_len, head_post
        )?;
        for (_, line) in *pre {
            writeln!(&mut out, "{line}")?;
        }
        for (_, line) in *group {
            writeln!(&mut out, "{line}")?;
        }
        for (_, line) in *post {
            writeln!(&mut out, "{line}")?;
        }
        Ok(())
    }
}

/// A bare diff for a single file. (A Patch file represents any number
/// of Diff instances.)
struct Diff<'a> {
    diff_line: &'a str,
    newfile_line: Option<&'a str>,
    #[allow(unused)]
    index_line: Option<&'a str>,
    minus_line: &'a str,
    plus_line: &'a str,
    hunks: Vec<Hunk<'a>>,
}

impl<'a> Diff<'a> {
    /// Not the head of the patch (i.e. mail headers / commit
    /// message), but of this diff. Ends with a newline.
    fn head(&self) -> String {
        let Self {
            diff_line,
            newfile_line,
            index_line: _,
            minus_line,
            plus_line,
            hunks: _,
        } = self;
        [
            Some(*diff_line),
            *newfile_line,
            Some(*minus_line),
            Some(*plus_line),
        ]
        .into_iter()
        .flatten()
        .join("\n")
            + "\n"
    }
}

fn parse_diff(diff: &str) -> Result<Diff<'_>> {
    let mut lines = diff.lines().enumerate();

    let (_, diff_line) = lines
        .next()
        .filter(|(_, l)| l.starts_with("diff "))
        .with_context(|| format!("invalid patch file format: missing 'diff ' line"))?;

    let (line0, line) = lines
        .next()
        .with_context(|| format!("invalid patch file format: unexpected EOF after 'diff ' line"))?;

    let (newfile_line, (line0, line)) = if line.starts_with("new file mode ") {
        (
            Some(line),
            lines.next().with_context(|| {
                format!("invalid patch file format: [missing `index` or `---` line]")
            })?,
        )
    } else {
        (None, (line0, line))
    };

    let (index_line, (line0, line)) = if line.starts_with("index ") {
        (
            Some(line),
            lines.next().with_context(|| {
                format!("invalid patch file format [missing `---` line]: {diff}")
            })?,
        )
    } else {
        (None, (line0, line))
    };

    if !line.starts_with("--- ") {
        bail!(
            "invalid patch file format: expected `---` on line {}",
            line0 + 1
        );
    }
    let minus_line = line;

    let (line0, line) = lines
        .next()
        .with_context(|| format!("invalid patch file format [missing `+++` line]: {diff}"))?;
    if !line.starts_with("+++ ") {
        bail!(
            "invalid patch file format: expected `+++` on line {}",
            line0 + 1
        );
    }
    let plus_line = line;

    let hunks = gather_hunks(lines);

    Ok(Diff {
        diff_line,
        newfile_line,
        index_line,
        minus_line,
        plus_line,
        hunks,
    })
}

fn gather_hunks<'s>(lines: impl Iterator<Item = (usize, &'s str)>) -> Vec<Hunk<'s>> {
    split_before(
        lines,
        |(_, line)| line.starts_with("@@ "),
        |group| Hunk { lines: group },
    )
}

fn split_hunk_into_changes<'a, 'h>(hunk: &'h Hunk<'a>) -> Result<Vec<Change<'a, 'h>>> {
    let (head_line_line0, head_line) = hunk
        .lines
        .first()
        .expect("hunks are expected to never be empty by construction");

    // XXX: are all these valid patterns? What happens if captures
    // fail?
    // @@ -0,0 +1,2 @@
    // @@ -42 42 @@
    // @@ -42 +1,2 @@
    // @@ -0,0 +1 @@
    let caps = re!(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? (.*)")
        .captures(head_line)
        .with_context(|| format!("invalid hunk head: {head_line}"))?;

    let mut orig_start: usize = caps.get_str_and_parse(1, *head_line_line0)?;
    let mut patched_start: usize = caps.get_str_and_parse(3, *head_line_line0)?;
    let head_post = caps.get_str(5);

    let mut remaining: &[(usize, &str)] = &hunk.lines[1..];
    let mut result = Vec::new();

    while !remaining.is_empty() {
        let (pre, rest) = take_while(remaining, |(_, l)| l.starts_with(' '));
        let (group, rest) = take_while(rest, |(_, l)| l.starts_with(['-', '+']));
        let (post, rest) = take_while(rest, |(_, l)| l.starts_with(' '));

        let pre_len = pre.len();

        let new_pre = if pre.len() > 3 {
            &pre[pre.len() - 3..]
        } else {
            pre
        };
        let new_pre_len = new_pre.len();

        let new_post = if post.len() > 3 { &post[..3] } else { post };
        let new_post_len = new_post.len();

        let group_minus_len = group.iter().filter(|(_, l)| l.starts_with('-')).count();
        // The group consists purely of lines starting with '-' and
        // '+' by its construction, hence:
        let group_plus_len = group.len() - group_minus_len;

        let orig_len = new_pre_len + group_minus_len + new_post_len;
        let patched_len = new_pre_len + group_plus_len + new_post_len;

        result.push(Change {
            orig_start,
            orig_len,
            patched_start,
            patched_len,
            head_post,
            pre: new_pre,
            group,
            post: new_post,
        });

        if rest.is_empty() {
            break;
        }

        orig_start += pre_len + group_minus_len;
        patched_start += pre_len + group_plus_len;

        remaining = rest;
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
