//! A bit of a hack to make creating `BString` instances easier, from a
//! mix of byte sequences and Display and Debug based format strings.
//!
//! Does not allocate, although it does use some indirections (could
//! be outputting more direct code as an optimization).
//!
//! The proper solution would be to make a proc macro that can parse
//! format strings and for `{}` formatting use wrappers that write
//! byte strings directly, but use Display for other types--except the
//! orphan rule would probably throw a wrench in here?
//!
//! # Examples
//!
//! ```
//! use patchparser::make_bstring;
//! let prefix = b"Hello ";
//! let i = 77;
//! let bstring = make_bstring!(
//!     {prefix} + {"World"} + {b"!"} + (" With {i} {}!", "balloons") + {&[0, 1, 2]}
//! );
//! assert_eq!(bstring, bstr::BStr::new("Hello World! With 77 balloons!\0\x01\x02"));
//! ```

use bstr::{BStr, BString, ByteVec};
use derive_more::From;

#[derive(From)]
pub enum FormatItem<'t> {
    Str(#[from] &'t str),
    Slice(#[from] &'t [u8]),
    Vec(#[from] Vec<u8>),
    VecRef(#[from] &'t Vec<u8>),
    BStr(#[from] &'t BStr),
    BString(#[from] BString),
    BStringRef(#[from] &'t BString),
    Writer(#[from] &'t dyn Fn(&mut Vec<u8>)),
}

impl<'t, const N: usize> From<&'t [u8; N]> for FormatItem<'t> {
    fn from(value: &'t [u8; N]) -> Self {
        value.as_slice().into()
    }
}

pub fn build_bstring<'t>(items: impl IntoIterator<Item = FormatItem<'t>>) -> BString {
    let mut output = BString::new(Vec::new());
    for item in items {
        match item {
            FormatItem::Str(s) => output.push_str(s),
            FormatItem::Slice(sl) => output.push_str(sl),
            FormatItem::Vec(sl) => output.push_str(sl),
            FormatItem::VecRef(sl) => output.push_str(sl),
            FormatItem::BStr(bstr) => output.push_str(bstr),
            FormatItem::BString(bstr) => output.push_str(bstr),
            FormatItem::BStringRef(bstr) => output.push_str(bstr),
            FormatItem::Writer(f) => f(&mut output),
        }
    }
    output
}

#[test]
fn t_build_bstring() {
    use std::io::Write;

    fn dynamic<F: Fn(&mut Vec<u8>)>(f: &F) -> &dyn Fn(&mut Vec<u8>) {
        f
    }

    let i = 77;
    let bs = build_bstring([
        "Hello".into(),
        b" World".into(),
        dynamic(&|out: &mut Vec<u8>| write!(out, ", {i}").unwrap()).into(),
    ]);
    assert_eq!(&bs, "Hello World, 77");
}

// #[macro_export]
// macro_rules! format_item {
//     { ! $($fmt:tt)* } => {
//         $crate::format_binary::FormatItem::Writer(&|out: &mut Vec<u8>| write!(out, $($fmt)*).unwrap())
//     };
//     { = $e:expr } => {
//         $crate::format_binary::FormatItem::from($e)
//     }
// }

/// (Do not use, internal use only.)
#[macro_export]
macro_rules! ___make_bstring {
    // End case
    {{} ; { }} => {
        bstr::BString::new(Vec::new())
    };
    {{} ; { , $($items:tt)* }} => {
        $crate::format_binary::build_bstring([ $($items)* ])
    };
    // Tail case
    {{ { $item:expr } } ; { $($items:tt)* }} => {
        $crate::___make_bstring!({ } ; {
            $($items)*,
            $crate::format_binary::FormatItem::from($item)
        })
    };
    {{ ( $($fmt:tt)+ ) } ; { $($items:tt)* }} => {
        $crate::___make_bstring!({ } ; {
            $($items)*,
            $crate::format_binary::FormatItem::Writer(&|out: &mut Vec<u8>| write!(out, $($fmt)*).unwrap())
        })
    };
    // Normal case
    {{ { $item:expr } + $($rest:tt)* } ; { $($items:tt)* }} => {
        $crate::___make_bstring!({ $($rest)* } ; {
            $($items)*,
            $crate::format_binary::FormatItem::from($item)
        })
    };
    {{ ( $($fmt:tt)+ ) + $($rest:tt)* } ; { $($items:tt)* }} => {
        $crate::___make_bstring!({ $($rest)* } ; {
            $($items)*,
            $crate::format_binary::FormatItem::Writer(&|out: &mut Vec<u8>| {
                use std::io::Write;
                write!(out, $($fmt)*).unwrap()
            })
        })
    }
}

/// Usage: use `+` to join segments, each of which can either be a
/// format string instance in round parens (which can only deal with
/// proper strings), or between curly braces any expression that
/// evaluates to a byte slice / vector or `BStr` / `BString` or normal
/// string, which is then added directly as bytes.
///
/// See example in the module docs.
#[macro_export]
macro_rules! make_bstring {
    { $($item:tt)* } => {
        $crate::___make_bstring!({ $($item)* }; {})
    }
}
