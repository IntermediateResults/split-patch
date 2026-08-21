use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
    process::exit,
};

use anyhow::anyhow;
use anyhow::{Context, Ok, Result};
use clap_with_warnings::clap_with_warnings;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use split_patch::{
    core::split_patch, path_utils::path_remove_common_lead, split_options::SplitArgs,
};

/// Test split patches in directory.
///
/// For all files in `input-dir`, split them with `split-patch` into
/// a subdir in output-base with a basename of the input file and
/// option appended.
#[clap_with_warnings]
#[derive(Debug, clap::Parser)]
#[command(version, about, long_about, allow_hyphen_values = true)]
pub struct TestArgs {
    /// Path to input directory
    #[clap(short, long, required = true)]
    input_dir: PathBuf,

    /// Base directory for output
    #[clap(short, long, required = true)]
    output_base: PathBuf,
}

fn main() -> Result<()> {
    let args = TestArgs::parse();

    let mut patch_files: Vec<PathBuf> = args
        .input_dir
        .read_dir()
        .with_context(|| anyhow!("reading input directory {:?}", args.input_dir))?
        .map(|result| result.map(|entry| entry.path()))
        .collect::<Result<_, std::io::Error>>()?;

    patch_files.sort();

    fs::create_dir_all(args.output_base.clone()).expect("creating output_base directory");

    let output_base = args.output_base;
    let common = || SplitArgs {
        monotonous_numbers: true,
        ..Default::default()
    };
    let split_args = [
        ("--", SplitArgs { ..common() }),
        (
            "--hunks",
            SplitArgs {
                hunks: true,
                ..common()
            },
        ),
        (
            "--changes",
            SplitArgs {
                changes: true,
                ..common()
            },
        ),
    ];
    let results: Vec<(&str, Vec<Result<()>>)> = split_args
        .par_iter()
        .map(|(option_string, split_args)| -> (&str, Vec<Result<()>>) {
            let results: Vec<Result<()>> = patch_files
                .par_iter()
                .map(|file| -> Result<()> {
                    let basename = file
                        .file_stem()
                        .with_context(|| anyhow!("extracting file stem from {file:?}"))?;
                    let full_output_dir = &output_base.join(option_string).join(basename);
                    fs::create_dir_all(full_output_dir).with_context(|| {
                        anyhow!("creating output directory {full_output_dir:?}")
                    })?;

                    let mut split_args = split_args.clone();
                    split_args.output_dir = Some(full_output_dir.clone());

                    let written = split_patch(file, &split_args.into())
                        .with_context(|| anyhow!("splitting the patch file {file:?}"))?;

                    let list_path = full_output_dir.join("_list");
                    (|| {
                        let mut out = BufWriter::new(File::create(&list_path)?);
                        for path in written {
                            let relative_path = path_remove_common_lead(&output_base, path)
                                .expect("beginnings are ensured to be identical");
                            out.write_all(relative_path.as_os_str().as_bytes())?;
                            out.write_all(b"\n")?;
                        }
                        Ok(())
                    })()
                    .with_context(|| anyhow!("writing to file {list_path:?}"))?;
                    Ok(())
                })
                .collect();
            (option_string, results)
        })
        .collect();

    results.iter().for_each(|(opt, results)| {
        results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .for_each(|err| {
                eprintln!("Failed with option {opt}: {err:#}");
            })
    });

    let tot_count: usize = results.iter().map(|(_opt, results)| results.len()).sum();
    let tot_count_alternative = split_args.len() * patch_files.len();
    assert_eq!(tot_count, tot_count_alternative);
    let err_count: usize = results
        .iter()
        .map(|(_opt, results)| results.iter().filter(|result| result.is_err()).count())
        .sum();

    println!("{err_count}/{tot_count} split-patch invocations failed");

    if err_count > 0 {
        exit(1);
    }

    Ok(())
}
