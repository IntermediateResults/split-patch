use std::path::PathBuf;

use clap::Parser;
use patchparser::patch::hunk::ParseMode;

#[derive(Debug, Clone, clap::Parser)]
#[command(allow_hyphen_values = true)]
pub struct SplitArgs {
    /// Split on hunk boundaries, not just file boundaries.
    #[clap(long)]
    pub hunks: bool,

    /// Split on individual change groups, too (implies `--hunks`)
    #[clap(short, long)]
    pub changes: bool,

    /// Always check everything (down to the individual changes) for
    /// correct syntax and change range numbers (unless
    /// `--ignore-range-errors` is given). By default, only parses
    /// fully on demand, i.e. when `--changes` is given.
    #[clap(long)]
    pub check: bool,

    /// Check everything like `--check`, but do not produce any output
    /// files.
    #[clap(long)]
    pub check_only: bool,

    /// Regenerate the output from the fully parsed version; by
    /// default, even with `--check`, by default the original data is
    /// re-used where possible. Implies `--check`.
    #[clap(short, long)]
    pub regenerate: bool,

    /// When parsing hunks (i.e. when `--check`, `--changes` or
    /// `--regenerate` was given), ignore range lengths in the input
    /// files. Range lengths used in the output files are always
    /// calculated from the actual change set bodies if hunks were
    /// parsed; this option simply omits a comparison with what is
    /// provided in the input files.
    #[clap(long)]
    pub ignore_range_errors: bool,

    /// Omit the addition of a prefix to the subject line of patch
    /// files that have a git style patch header
    #[clap(long)]
    pub no_subject_change: bool,

    /// When using `--changes`, use a single number counter for
    /// generating the ids for the generated output file names and
    /// subject prefixes instead of `{hunk_id}-{change_id}`.
    #[clap(long)]
    pub monotonous_numbers: bool,

    /// Do not produce any output files (useful for checks).
    #[clap(long)]
    pub dry_run: bool,

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
    /// Path to the directory where to write the split files
    /// to. Default: the same directory as the input file.
    pub output_dir: Option<PathBuf>,
    /// Do not produce any output files (useful for checks).
    pub dry_run: bool,
    /// Which boundary to split on.
    pub mode: SplitMode,
    /// Add a prefix to the subject line of patch files if present
    pub subject_change: bool,
    /// When using `--changes`, use a single number counter for
    /// generating the ids for the generated output file names and
    /// subject prefixes instead of `{hunk_id}-{change_id}`.
    pub monotonous_numbers: bool,
    /// Insert the prefix after "[PATCH]" instead of before
    /// everything.
    pub insert_after_patch: bool,
    pub parse_mode: ParseMode,
    /// When parsing hunks, ignore range lengths in the input
    /// files. Range lengths used in the output files are always
    /// calculated from the actual change set bodies if hunks were
    /// parsed; this option simply omits a comparison with what is
    /// provided in the input files.
    pub ignore_range_errors: bool,
}

impl From<SplitArgs> for SplitOptions {
    fn from(value: SplitArgs) -> Self {
        let SplitArgs {
            hunks,
            changes,
            check,
            check_only,
            regenerate,
            ignore_range_errors,
            no_subject_change,
            monotonous_numbers,
            dry_run,
            output_dir,
            no_insert_after_patch,
        } = value;

        let full_check = check || check_only;
        let parse_mode = ParseMode::from_options(full_check, regenerate);

        SplitOptions {
            output_dir,
            dry_run: check_only || dry_run,
            mode: if changes {
                SplitMode::Change
            } else if hunks {
                SplitMode::Hunk
            } else {
                SplitMode::File
            },
            subject_change: !no_subject_change,
            monotonous_numbers,
            insert_after_patch: !no_insert_after_patch,
            parse_mode,
            ignore_range_errors,
        }
    }
}
