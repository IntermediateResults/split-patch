use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Clone, clap::Parser)]
#[command(allow_hyphen_values = true)]
pub struct SplitArgs {
    /// Split on hunk boundaries, not just file boundaries.
    #[clap(long)]
    pub hunks: bool,

    /// Split on individual change groups, too (implies `--hunks`)
    #[clap(short, long)]
    pub changes: bool,

    /// Omit the addition of a prefix to the subject line of patch
    /// files that have a git style patch header
    #[clap(long)]
    pub no_subject_change: bool,

    /// When using `--changes`, use a single number counter for
    /// generating the ids for the generated output file names and
    /// subject prefixes instead of `{hunk_id}-{change_id}`.
    #[clap(long)]
    pub monotonous_numbers: bool,

    /// Path to the directory where to write the split files
    /// to. Default: the same directory as the input file.
    #[clap(long)]
    pub output_dir: Option<PathBuf>,

    /// Do not insert the prefix after "[PATCH]", but before
    /// everything.
    #[clap(long)]
    pub no_insert_after_patch: bool,
}

impl Default for SplitArgs {
    fn default() -> Self {
        SplitArgs::parse_from(["ignored-program-name"])
    }
}

#[test]
fn t_default_split_args() {
    let d = SplitArgs::default();
    assert!(!d.no_insert_after_patch);
}

pub enum SplitMode {
    File,
    Hunk,
    Change,
}

impl SplitMode {
    /// Whether to split on hunks (implied when splitting on changes).
    pub fn hunks(&self) -> bool {
        match self {
            SplitMode::File => false,
            SplitMode::Hunk | SplitMode::Change => true,
        }
    }

    /// Whether to split on changes
    pub fn changes(&self) -> bool {
        match self {
            SplitMode::File | SplitMode::Hunk => false,
            SplitMode::Change => true,
        }
    }
}

/// The options for splitting a patch file (can be converted from
/// `SplitArgs`)
pub struct SplitOptions {
    pub mode: SplitMode,
    /// Add a prefix to the subject line of patch files if present
    pub subject_change: bool,
    /// When using `--changes`, use a single number counter for
    /// generating the ids for the generated output file names and
    /// subject prefixes instead of `{hunk_id}-{change_id}`.
    pub monotonous_numbers: bool,
    /// Path to the directory where to write the split files
    /// to. Default: the same directory as the input file.
    pub output_dir: Option<PathBuf>,
    /// Insert the prefix after "[PATCH]" instead of before
    /// everything.
    pub insert_after_patch: bool,
}

impl From<SplitArgs> for SplitOptions {
    fn from(value: SplitArgs) -> Self {
        let SplitArgs {
            hunks,
            changes,
            no_subject_change,
            monotonous_numbers,
            output_dir,
            no_insert_after_patch,
        } = value;

        SplitOptions {
            mode: if changes {
                SplitMode::Change
            } else if hunks {
                SplitMode::Hunk
            } else {
                SplitMode::File
            },
            subject_change: !no_subject_change,
            monotonous_numbers,
            output_dir,
            insert_after_patch: !no_insert_after_patch,
        }
    }
}
