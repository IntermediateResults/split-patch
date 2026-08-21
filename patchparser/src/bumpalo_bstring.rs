use std::ops::{Deref, DerefMut};

use bstr::BStr;
use bumpalo::{collections::Vec, Bump};

/// like `bstr::BString` but allocating from `bumpalo::Bump`
pub struct BString<'bump>(Vec<'bump, u8>);

impl<'bump> BString<'bump> {
    pub fn new_in(bump: &'bump Bump) -> Self {
        Self(Vec::new_in(bump))
    }

    #[must_use]
    pub fn into_bump_slice(self) -> &'bump BStr {
        self.0.into_bump_slice().as_ref()
    }
}

impl<'bump> Deref for BString<'bump> {
    type Target = Vec<'bump, u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'bump> DerefMut for BString<'bump> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
