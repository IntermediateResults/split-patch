use std::path::PathBuf;

use clap_with_warnings::clap_with_warnings;

#[derive(Debug, clap::Args)]
#[command(allow_hyphen_values = true)]
pub struct SplitOptions {
    /// Split on hunk boundaries, not just file boundaries.
    // This field is private because it is implied by changes, thus
    // must only be accessible by accessor method.
    #[clap(long)]
    hunks: bool,

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

impl SplitOptions {
    /// Whether to split on hunks. Is true even if the user (only)
    /// specified `--changes` (which implies to split on hunks, too)
    pub fn hunks(&self) -> bool {
        self.changes || self.hunks
    }
}

/// Split the given patchfile(s) into new files
///
/// So that each new file only contains the part of the patch for
/// one particular target file.
#[clap_with_warnings]
#[derive(Debug, clap::Parser)]
#[command(version, about, long_about, allow_hyphen_values = true)]
pub struct Args {
    /// Path(s) to patch file(s)
    #[clap(required = true)]
    pub patch_file: Vec<PathBuf>,

    #[clap(flatten)]
    pub split_options: SplitOptions,

    /// Do not print the list of generated files.
    #[clap(short, long)]
    pub quiet: bool,
}
