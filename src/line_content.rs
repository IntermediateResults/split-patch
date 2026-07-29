use std::{mem::transmute, sync::OnceLock};

use anyhow::Result;
use bstr::{BString, ByteSlice};
use bumpalo::Bump;

use crate::{anyhow_once::AnyhowOnce, line::Line};

pub trait FromLines<'t>: Sized {
    fn from_lines(lines: &'t [Line<'t>], bump: &'t Bump) -> Result<Self, anyhow::Error>;
}

/// Owned content that can be represented as lines and then by T,
/// lazily
pub struct LineContent<T> {
    content: BString,
    bump: Bump,
    __unsafe_lines: OnceLock<Vec<Line<'static>>>,
    __unsafe_parsed_result: OnceLock<Result<T, AnyhowOnce>>,
}

impl<T> Drop for LineContent<T> {
    fn drop(&mut self) {
        self.__unsafe_parsed_result.take();
        self.__unsafe_lines.take();
        self.bump.reset();
    }
}

impl<T> LineContent<T> {
    pub fn from_content(content: BString) -> Self {
        Self {
            content,
            bump: Bump::new(),
            __unsafe_lines: OnceLock::new(),
            __unsafe_parsed_result: OnceLock::new(),
        }
    }

    pub fn content(&self) -> &[u8] {
        &self.content
    }

    pub fn lines<'s>(&'s self) -> &'s [Line<'s>] {
        let lines = self.__unsafe_lines.get_or_init(|| {
            let lines: Vec<Line<'s>> = self
                .content
                .lines()
                .enumerate()
                .map(Line::from_tuple)
                .collect();
            unsafe {
                // Safe because only the life time is modified and no
                // public access is given to the 'static version
                transmute::<Vec<Line<'s>>, Vec<Line<'static>>>(lines)
            }
        });
        unsafe {
            // Safe because the referenced `content` is on the heap
            // and never modified once created, hence doesn't change
            // address, and can't be deallocated for the duration of
            // 's.
            transmute::<&[Line<'static>], &'s [Line<'s>]>(&**lines)
        }
    }

    /// Give public access to the field (needed for the
    /// `def_line_content_for!` macro), but as `unsafe` function only
    pub unsafe fn __unsafe_parsed_result(&self) -> &OnceLock<Result<T, AnyhowOnce>> {
        &self.__unsafe_parsed_result
    }

    /// Access to the allocator
    pub fn bump(&self) -> &Bump {
        &self.bump
    }
}

/// Given a type T that has a life time, define a wrapper type that
/// owns the content as byte vector, lines derived on demand from it,
/// and T derived on demand from the lines via the `FromLines` trait.
///
/// You must provide the type name *without* the angle brackets and
/// life time argument. The type must *only* take a single life time
/// argument as type parameters.
#[macro_export]
macro_rules! def_line_content_for {
    { $name:ident, $($T:tt)* } => {
        pub struct $name($crate::line_content::LineContent<$($T)*<'static>>);

        impl From<::bstr::BString> for $name
        where
            for<'a> $($T)*<'a>: FromLines<'a>
        {
            fn from(content: ::bstr::BString) -> Self {
                Self::from_content(content)
            }
        }

        impl $name
        where
            for<'a> $($T)*<'a>: FromLines<'a>
        {
            pub fn from_content(content: ::bstr::BString) -> Self {
                Self($crate::line_content::LineContent::from_content(content))
            }

            pub fn from_path(path: &::std::path::Path) -> Result<Self> {
                Ok(Self::from_content(
                    ::bstr::BString::new(std::fs::read(path).context("reading file")?)
                ))
            }

            pub fn content(&self) -> &[u8] {
                self.0.content()
            }

            pub fn lines(&self) -> &[Line<'_>] {
                self.0.lines()
            }

            pub fn parsed<'s>(&'s self) -> Result<&'s $($T)*<'s>, ::anyhow::Error> {
                use ::std::mem::transmute;

                let parsed_result = unsafe {
                    // Safe because the life time is changed to 's below
                    self.0.__unsafe_parsed_result()
                }.get_or_init(|| {
                    let lines = self.0.lines();
                    let parsed_result: Result<$($T)*<'s>, $crate::anyhow_once::AnyhowOnce>
                        = $($T)*::from_lines(lines, self.0.bump()).map_err(Into::into);
                    unsafe {
                        // Safe because only the life time is modified and no
                        // public access is given to the 'static version
                        transmute::<
                            Result<$($T)*<'s>, $crate::anyhow_once::AnyhowOnce>,
                            Result<$($T)*<'static>, $crate::anyhow_once::AnyhowOnce>
                        >(parsed_result)
                    }
                });

                match parsed_result {
                    Ok(parsed) => {
                        let parsed = unsafe {
                            // Safe because the referenced `lines` and
                            // `content` are on the heap and never modified
                            // once created, hence don't change address, and
                            // can't be deallocated for the duration of 's.
                            transmute::<
                                &$($T)*<'static>,
                                &'s $($T)*<'s>
                            >(parsed)
                        };
                        Ok(parsed)
                    },
                    Err(e) => {
                        Err(e.take())
                    }
                }
            }
        }
    }
}
