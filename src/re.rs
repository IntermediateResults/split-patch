use std::{any::type_name, error::Error, str::FromStr};

use anyhow::{Context, Result};
use regex::bytes::Captures;

/// Expands to code that instantiates a `regex::Regex` instance on
/// first access, but caches it for the remainder of the life of the
/// program (i.e. returns `&'static Regex`). Note that this panics for
/// invalid regular expressions when accessed.
#[macro_export]
macro_rules! re {
    { $regex_string:expr } => {
        {
            ::lazy_static::lazy_static!{
                static ref RE: ::regex::bytes::Regex = ::regex::bytes::Regex::new($regex_string).unwrap();
            }
            &*RE
        }
    }
}

pub trait GetStr<'t> {
    fn get_str(&self, i: usize) -> &'t [u8];

    fn get_str_then_parse<T: FromStr>(&self, i: usize, line0: usize) -> Result<T>
    where
        <T as FromStr>::Err: Send + Sync + Error + 'static;
}

impl<'t> GetStr<'t> for Captures<'t> {
    /// Panics if i is outside the range of available captures
    fn get_str(&self, i: usize) -> &'t [u8] {
        self.get(i)
            .expect("expected capture to be present")
            .as_bytes()
    }

    /// Panics if i is outside the range of available captures. Parses
    /// the capture to the result type, showing the line number in the
    /// error message if failing to parse.
    fn get_str_then_parse<T: FromStr>(&self, i: usize, line0: usize) -> Result<T>
    where
        <T as FromStr>::Err: Send + Sync + Error + 'static,
    {
        let bs = self.get_str(i);
        let s = std::str::from_utf8(bs).with_context(|| {
            format!("parsing into {}: not a string: {:?}", type_name::<T>(), bs)
        })?;
        s.parse()
            .with_context(|| format!("parsing capture {i} on line {}", line0 + 1))
    }
}
