//! Utilities that exclusively allocate memory from the bumpalo memory
//! allocator

use bumpalo::{collections as bc, Bump};

/// Adapted copy of `split_before` for bumpalo allocation
//
// Can't find anything in itertools; coalesce doesn't allow to build
// groups of different type than the item type. Don't need it to be
// lazy, avoids the need for generators.
pub fn split_before_in<'a, 'b, T, G>(
    items: &'a [T],
    is_boundary: impl Fn(&'a T) -> bool,
    group_constructor: impl Fn(&'a [T]) -> G,
    bump: &'b Bump,
) -> bc::Vec<'b, G> {
    let finish_group = |groups: &mut bc::Vec<G>, current_group: &'a [T]| {
        if !current_group.is_empty() {
            groups.push(group_constructor(current_group));
        }
    };

    let mut groups = bc::Vec::new_in(bump);
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

    fn t<'a, 'b, 's>(items: &'a [&'s str], bump: &'b Bump) -> bc::Vec<'b, &'a [&'s str]> {
        split_before_in(items, |s: &&str| s.starts_with("@"), |v| v, bump)
    }

    let b = &Bump::new();

    assert_eq!(t(&["@c"], b), [vec!["@c"]],);
    // With no lines, no group should be created
    assert_eq!(t(&[], b), typed::<[Vec<&str>; 0]>([]),);
    // Not sure how it should behave for this one
    assert_eq!(
        t(&["a", "b", "@c", "d", "@e", "f", "g"], b),
        [vec!["a", "b"], vec!["@c", "d"], vec!["@e", "f", "g"],]
    );
    // The normal cases, right?
    assert_eq!(
        t(&["@a", "b", "@c", "d", "@e", "f", "g"], b),
        [vec!["@a", "b"], vec!["@c", "d"], vec!["@e", "f", "g"],]
    );
    assert_eq!(
        t(&["@a", "b", "@c", "d", "@e"], b),
        [vec!["@a", "b"], vec!["@c", "d"], vec!["@e"],]
    );
    assert_eq!(
        t(&["@a", "@b", "@c", "d", "@e"], b),
        [vec!["@a"], vec!["@b"], vec!["@c", "d"], vec!["@e"],]
    );
}

/// Variant of `split_before_in` that stops on errors
pub fn try_split_before_in<'a, 'b, T, G, E>(
    items: &'a [T],
    mut is_boundary: impl FnMut(&'a T) -> bool,
    mut group_constructor: impl FnMut(&'a [T]) -> Result<G, E>,
    bump: &'b Bump,
) -> Result<bc::Vec<'b, G>, E> {
    let mut finish_group = |groups: &mut bc::Vec<G>, current_group: &'a [T]| -> Result<(), E> {
        if !current_group.is_empty() {
            groups.push(group_constructor(current_group)?);
        }
        Ok(())
    };

    let mut groups = bc::Vec::new_in(bump);
    let mut current_group_start = 0;
    for (i, item) in items.iter().enumerate() {
        if is_boundary(item) {
            finish_group(&mut groups, &items[current_group_start..i])?;
            current_group_start = i;
        }
    }
    finish_group(&mut groups, &items[current_group_start..])?;

    Ok(groups)
}
