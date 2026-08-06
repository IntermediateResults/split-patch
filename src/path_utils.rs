use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};

pub fn path_remove_common_lead<P1: AsRef<Path>, P2: AsRef<Path>>(
    shorter: P1,
    longer: P2,
) -> Result<PathBuf> {
    let firsts = shorter.as_ref().components();
    let mut longers = longer.as_ref().components();
    for f in firsts {
        let s = longers
            .next()
            .ok_or_else(|| anyhow!("longer path is shorter than shorter path"))?;
        if f != s {
            bail!("path beginnings are not equal");
        }
    }
    let mut path = PathBuf::from("");
    for segment in longers {
        path.push(segment);
    }
    Ok(path)
}

#[test]
fn t_path_remove_common_lead() -> Result<()> {
    let t = path_remove_common_lead;
    let p = PathBuf::from;

    assert_eq!(t("/foo", "/foo")?, p(""));
    assert_eq!(t("/foo", "/foo/")?, p(""));
    assert_eq!(t("/foo", "/foo/bar")?, p("bar"));
    assert_eq!(t("/foo", "/foo/bar/")?, p("bar/"));
    assert_eq!(t("/foo", "/foo/bar/baz")?, p("bar/baz"));
    assert_eq!(
        t("/foo/bar", "/foo").err().unwrap().to_string(),
        "longer path is shorter than shorter path"
    );

    Ok(())
}
