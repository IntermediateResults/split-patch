use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub fn take_while<'a, T>(lines: &'a [T], predicate: impl Fn(&T) -> bool) -> (&'a [T], &'a [T]) {
    let count = lines.iter().take_while(|l| predicate(l)).count();

    lines.split_at(count)
}

/// This does not add a file extension, but adds a suffix to the file
/// name *before* the existing and unchanged file extension
pub fn add_suffix(path: &Path, addon: &str) -> Result<PathBuf> {
    let file_name = {
        let mut stem = path
            .file_stem()
            .context("failed to extract file stem from path")?
            .to_owned();
        stem.push(addon);
        if let Some(ext) = path.extension() {
            stem.push(".");
            stem.push(ext);
        }
        stem
    };
    if let Some(parent) = path.parent() {
        Ok(parent.join(file_name))
    } else {
        Ok(PathBuf::from(file_name))
    }
}

#[test]
fn t_add_suffix() {
    let t = |path: &str, addon| -> String {
        add_suffix(path.as_ref(), addon)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(t("foo.png", "-123"), "foo-123.png");
    assert_eq!(t("bar/baz/foo.png", "-123"), "bar/baz/foo-123.png");
}
