use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

pub fn take_while<'a, T>(lines: &'a [T], predicate: impl Fn(&T) -> bool) -> (&'a [T], &'a [T]) {
    let count = lines.iter().take_while(|l| predicate(l)).count();

    lines.split_at(count)
}

/// This does not add a file extension, but adds a suffix to the file
/// name *before* the existing and unchanged file extension
pub fn add_suffix(path: &Path, addon: &OsStr) -> Result<PathBuf> {
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
    let t = |path: &str, addon: &str| -> String {
        add_suffix(path.as_ref(), addon.as_ref())
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(t("foo.png", "-123"), "foo-123.png");
    assert_eq!(t("bar/baz/foo.png", "-123"), "bar/baz/foo-123.png");
    assert_eq!(t("foo", "-123"), "foo-123");
}

// Can't find anything in itertools; coalesce doesn't allow to build
// groups of different type than the item type. Don't need it to be
// lazy, avoids the need for generators.
pub fn split_before<'a, T, G>(
    items: &'a [T],
    is_boundary: impl Fn(&'a T) -> bool,
    group_constructor: impl Fn(&'a [T]) -> G,
) -> Vec<G> {
    let finish_group = |groups: &mut Vec<G>, current_group: &'a [T]| {
        if !current_group.is_empty() {
            groups.push(group_constructor(current_group));
        }
    };

    let mut groups = Vec::new();
    let mut current_group_start = 0;
    for (i, item) in items.iter().enumerate() {
        if is_boundary(item) {
            finish_group(&mut groups, &items[current_group_start..i]);
            current_group_start = i;
        }
    }
    finish_group(&mut groups, &items[current_group_start..]);

    groups
}

#[test]
fn t_split_before() {
    fn typed<T>(val: T) -> T {
        val
    }

    fn t<'a, 's>(items: &'a [&'s str]) -> Vec<&'a [&'s str]> {
        split_before(items, |s: &&str| s.starts_with("@"), |v| v)
    }

    assert_eq!(t(&["@c"]), [vec!["@c"]],);
    // With no lines, no group should be created
    assert_eq!(t(&[]), typed::<[Vec<&str>; 0]>([]),);
    // Not sure how it should behave for this one
    assert_eq!(
        t(&["a", "b", "@c", "d", "@e", "f", "g"]),
        [vec!["a", "b"], vec!["@c", "d"], vec!["@e", "f", "g"],]
    );
    // The normal cases, right?
    assert_eq!(
        t(&["@a", "b", "@c", "d", "@e", "f", "g"]),
        [vec!["@a", "b"], vec!["@c", "d"], vec!["@e", "f", "g"],]
    );
    assert_eq!(
        t(&["@a", "b", "@c", "d", "@e"]),
        [vec!["@a", "b"], vec!["@c", "d"], vec!["@e"],]
    );
    assert_eq!(
        t(&["@a", "@b", "@c", "d", "@e"]),
        [vec!["@a"], vec!["@b"], vec!["@c", "d"], vec!["@e"],]
    );
}
