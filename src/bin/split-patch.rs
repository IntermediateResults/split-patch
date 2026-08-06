use std::{
    io::{stdout, BufWriter, Write},
    os::unix::ffi::OsStrExt,
};

use anyhow::{anyhow, Context, Result};

use split_patch::{args::Args, core::split_patch};

fn main() -> Result<()> {
    let args = Args::parse();

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
