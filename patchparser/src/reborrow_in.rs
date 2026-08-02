use bumpalo::Bump;

/// Unlike `CloneIn`, this allows to change the type; implementors are
/// meant to only change lifetime parameters of the type.
pub trait ReborrowIn<'b>: Sized {
    type Reborrowed: 'b;

    fn reborrow_in(&self, bump: &'b Bump) -> Self::Reborrowed;
}
