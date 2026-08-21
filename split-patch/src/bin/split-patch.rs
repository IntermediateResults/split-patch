use std::{
    io::{stdout, BufWriter, Write},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
};

use anyhow::{anyhow, Context, Result};
use clap::Parser;

use split_patch::{clap_styles::clap_styles, core::split_patch, split_options::SplitArgs};

/// Split the given patchfile(s) into new files
///
/// So that each new file only contains the part of the patch for
/// one particular target file, or even only one hunk or change.
#[derive(Debug, Clone, clap::Parser)]
#[command(
    version,
    about,
    long_about,
    allow_hyphen_values = true,
    next_line_help = true,
    styles = clap_styles(),
)]
pub struct Args {
    /// Path(s) to patch file(s)
    #[clap(required = true)]
    pub patch_file: Vec<PathBuf>,

    #[clap(flatten)]
    pub split_args: SplitArgs,

    /// Do not print the list of generated files.
    #[clap(short, long)]
    pub quiet: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let split_options = args.split_args.into();

    for patch_file in &args.patch_file {
        let written = split_patch(patch_file, &split_options)
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
            .context("writing to stdout")?;
        }
    }

    Ok(())
}
