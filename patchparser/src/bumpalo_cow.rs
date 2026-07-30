//! A copy of `std::borrow::Cow` adapted for allocation from bumpalo

use core::fmt;
// #[cfg(not(no_global_oom_handling))]
use std::{
    borrow::Borrow,
    cmp::Ordering,
    hash::{Hash, Hasher},
    ops::Deref,
};

use bumpalo::{collections as bc, Bump};

pub trait ToOwnedIn<'b> {
    type Owned: Borrow<Self>;

    fn to_owned_in(&self, bump: &'b Bump) -> Self::Owned;

    fn clone_into_in(&self, target: &mut Self::Owned, bump: &'b Bump) {
        *target = self.to_owned_in(bump);
    }
}

pub trait CloneIn<'b>: Sized {
    fn clone_in(&self, bump: &'b Bump) -> Self;

    fn clone_from_in(&mut self, source: &Self, bump: &'b Bump)
    // where
    //     Self: ~const Destruct,
    {
        *self = source.clone_in(bump)
    }
}

pub enum BumpaloCow<'b, 'a, B: ?Sized + 'a>
where
    B: ToOwnedIn<'b>,
{
    Borrowed(&'a B),
    Owned(<B as ToOwnedIn<'b>>::Owned),
}

impl<'b, 'a, B: ?Sized + 'a> From<&'a B> for BumpaloCow<'b, 'a, B>
where
    B: ToOwnedIn<'b>,
{
    fn from(value: &'a B) -> Self {
        BumpaloCow::Borrowed(value)
    }
}

// Conflicting implementations with From<T> for T in core.
// impl<'b, 'a, B: ?Sized + 'a> From<<B as ToOwnedIn<'b>>::Owned> for BumpaloCow<'b, 'a, B>
// where
//     B: ToOwnedIn<'b>,
// {
//     fn from(value: <B as ToOwnedIn<'b>>::Owned) -> Self {
//         BumpaloCow::Owned(value)
//     }
// }

impl<'b, B: ?Sized + ToOwnedIn<'b>> CloneIn<'b> for BumpaloCow<'b, '_, B> {
    fn clone_in(&self, bump: &'b Bump) -> Self {
        use BumpaloCow::*;
        match *self {
            Borrowed(b) => Borrowed(b),
            Owned(ref o) => {
                let b: &B = o.borrow();
                Owned(b.to_owned_in(bump))
            }
        }
    }

    fn clone_from_in(&mut self, source: &Self, bump: &'b Bump) {
        use BumpaloCow::*;
        match (self, source) {
            (&mut Owned(ref mut dest), &Owned(ref o)) => o.borrow().clone_into_in(dest, bump),
            (t, s) => *t = s.clone_in(bump),
        }
    }
}

impl<'b, B: ?Sized + ToOwnedIn<'b>> BumpaloCow<'b, '_, B> {
    pub const fn is_borrowed(&self) -> bool {
        use BumpaloCow::*;
        match *self {
            Borrowed(_) => true,
            Owned(_) => false,
        }
    }

    pub const fn is_owned(&self) -> bool {
        !self.is_borrowed()
    }

    pub fn to_mut(&mut self, bump: &'b Bump) -> &mut <B as ToOwnedIn<'b>>::Owned {
        use BumpaloCow::*;
        match *self {
            Borrowed(borrowed) => {
                *self = Owned(borrowed.to_owned_in(bump));
                match *self {
                    Borrowed(..) => unreachable!(),
                    Owned(ref mut owned) => owned,
                }
            }
            Owned(ref mut owned) => owned,
        }
    }

    pub fn into_owned_in(self, bump: &'b Bump) -> <B as ToOwnedIn<'b>>::Owned {
        use BumpaloCow::*;
        match self {
            Borrowed(borrowed) => borrowed.to_owned_in(bump),
            Owned(owned) => owned,
        }
    }
}

impl<'b, B: ?Sized + ToOwnedIn<'b>> Deref for BumpaloCow<'b, '_, B>
where
    B::Owned: Borrow<B>,
{
    type Target = B;

    fn deref(&self) -> &B {
        use BumpaloCow::*;
        match *self {
            Borrowed(borrowed) => borrowed,
            Owned(ref owned) => owned.borrow(),
        }
    }
}

impl<'b, B: ?Sized> Eq for BumpaloCow<'b, '_, B> where B: Eq + ToOwnedIn<'b> {}

impl<'b, B: ?Sized> Ord for BumpaloCow<'b, '_, B>
where
    B: Ord + ToOwnedIn<'b>,
{
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        Ord::cmp(&**self, &**other)
    }
}

impl<'bump, 'a, 'b, B: ?Sized, C: ?Sized> PartialEq<BumpaloCow<'bump, 'b, C>>
    for BumpaloCow<'bump, 'a, B>
where
    B: PartialEq<C> + ToOwnedIn<'bump>,
    C: ToOwnedIn<'bump>,
{
    #[inline]
    fn eq(&self, other: &BumpaloCow<'bump, 'b, C>) -> bool {
        PartialEq::eq(&**self, &**other)
    }
}

impl<'bump, 'a, B: ?Sized> PartialOrd for BumpaloCow<'bump, 'a, B>
where
    B: PartialOrd + ToOwnedIn<'bump>,
{
    // XXX this is wrong, no requirement for the same 'bump, please
    #[inline]
    fn partial_cmp(&self, other: &BumpaloCow<'bump, 'a, B>) -> Option<Ordering> {
        PartialOrd::partial_cmp(&**self, &**other)
    }
}

impl<'bump, B: ?Sized> fmt::Debug for BumpaloCow<'bump, '_, B>
where
    B: fmt::Debug + ToOwnedIn<'bump, Owned: fmt::Debug>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use BumpaloCow::*;
        match *self {
            Borrowed(ref b) => fmt::Debug::fmt(b, f),
            Owned(ref o) => fmt::Debug::fmt(o, f),
        }
    }
}

impl<'bump, B: ?Sized> fmt::Display for BumpaloCow<'bump, '_, B>
where
    B: fmt::Display + ToOwnedIn<'bump, Owned: fmt::Display>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use BumpaloCow::*;
        match *self {
            Borrowed(ref b) => fmt::Display::fmt(b, f),
            Owned(ref o) => fmt::Display::fmt(o, f),
        }
    }
}

impl<'bump, B: ?Sized> Default for BumpaloCow<'bump, '_, B>
where
    B: ToOwnedIn<'bump, Owned: Default>,
{
    /// Creates an owned Cow<'a, B> with the default value for the contained owned value.
    fn default() -> Self {
        use BumpaloCow::*;
        Owned(<B as ToOwnedIn>::Owned::default())
    }
}

impl<'bump, B: ?Sized> Hash for BumpaloCow<'bump, '_, B>
where
    B: Hash + ToOwnedIn<'bump>,
{
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(&**self, state)
    }
}

impl<'bump, T: ?Sized + ToOwnedIn<'bump>> AsRef<T> for BumpaloCow<'bump, '_, T> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl<'bump> ToOwnedIn<'bump> for str {
    type Owned = bc::String<'bump>;

    fn to_owned_in(&self, bump: &'bump Bump) -> Self::Owned {
        bc::String::from_str_in(self, bump)
    }
}

// Can't implement these as there is no access to bump; or should we
// store that inside BorrowCow?

// impl<'bump, 'a> Add<&'a str> for BumpaloCow<'bump, 'a, str> {
//     type Output = BumpaloCow<'bump, 'a, str>;

//     #[inline]
//     fn add(mut self, rhs: &'a str) -> Self::Output {
//         self += rhs;
//         self
//     }
// }

// impl<'bump, 'a> Add<BumpaloCow<'bump, 'a, str>> for BumpaloCow<'bump, 'a, str> {
//     type Output = BumpaloCow<'bump, 'a, str>;

//     #[inline]
//     fn add(mut self, rhs: BumpaloCow<'bump, 'a, str>) -> Self::Output {
//         self += rhs;
//         self
//     }
// }

// // #[cfg(not(no_global_oom_handling))]
// impl<'bump, 'a> AddAssign<&'a str> for BumpaloCow<'bump, 'a, str> {
//     fn add_assign(&mut self, rhs: &'a str) {
//         if self.is_empty() {
//             *self = BumpaloCow::Borrowed(rhs)
//         } else if !rhs.is_empty() {
//             if let BumpaloCow::Borrowed(lhs) = *self {
//                 let mut s = bc::String::with_capacity_in(lhs.len() + rhs.len(), bump);
//                 s.push_str(lhs);
//                 *self = BumpaloCow::Owned(s);
//             }
//             self.to_mut().push_str(rhs);
//         }
//     }
// }

// // #[cfg(not(no_global_oom_handling))]
// impl<'bump, 'a> AddAssign<BumpaloCow<'bump, 'a, str>> for BumpaloCow<'bump, 'a, str> {
//     fn add_assign(&mut self, rhs: BumpaloCow<'bump, 'a, str>) {
//         if self.is_empty() {
//             *self = rhs
//         } else if !rhs.is_empty() {
//             if let BumpaloCow::Borrowed(lhs) = *self {
//                 let mut s = String::with_capacity(lhs.len() + rhs.len());
//                 s.push_str(lhs);
//                 *self = BumpaloCow::Owned(s);
//             }
//             self.to_mut().push_str(&rhs);
//         }
//     }
// }

impl<'a, T: 'a + CloneIn<'a>> ToOwnedIn<'a> for [T] {
    type Owned = bc::Vec<'a, T>;

    fn to_owned_in(&self, bump: &'a Bump) -> Self::Owned {
        bc::Vec::from_iter_in(self.iter().map(|v| v.clone_in(bump)), bump)
    }
}
