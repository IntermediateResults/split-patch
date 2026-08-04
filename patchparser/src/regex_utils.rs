use std::{any::type_name, error::Error, str::FromStr};

use anyhow::{Context, Result};
use regex::bytes::Captures;

use crate::line::Line;

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
    fn get_str(&self, i: usize) -> Option<&'t [u8]>;

    fn str(&self, i: usize) -> &'t [u8];

    fn get_str_then_parse<'l, T: FromStr>(&self, i: usize, in_line: Line<'l>) -> Result<Option<T>>
    where
        <T as FromStr>::Err: Send + Sync + Error + 'static;

    fn str_then_parse<'l, T: FromStr>(&self, i: usize, in_line: Line<'l>) -> Result<T>
    where
        <T as FromStr>::Err: Send + Sync + Error + 'static;
}

impl<'t> GetStr<'t> for Captures<'t> {
    /// Returns `None` if there is no capture with number `i`
    fn get_str(&self, i: usize) -> Option<&'t [u8]> {
        self.get(i).map(|c| c.as_bytes())
    }

    /// Panics if there is no capture with number `i`
    fn str(&self, i: usize) -> &'t [u8] {
        self.get(i)
            .expect("expected capture to be present")
            .as_bytes()
    }

    /// Panics if i is outside the range of available captures. Parses
    /// the capture to the result type, showing the line number in the
    /// error message if failing to parse.
    fn get_str_then_parse<'l, T: FromStr>(&self, i: usize, in_line: Line<'l>) -> Result<Option<T>>
    where
        <T as FromStr>::Err: Send + Sync + Error + 'static,
    {
        let bs = match self.get_str(i) {
            Some(v) => v,
            None => return Ok(None),
        };
        let s = std::str::from_utf8(bs).with_context(|| {
            format!("parsing into {}: not a string: {:?}", type_name::<T>(), bs)
        })?;
        s.parse()
            .with_context(|| format!("parsing capture {i} on line {in_line}"))
            .map(Some)
    }

    /// Panics if there is no capture with number `i`. Parses
    /// the capture to the result type, showing the line number in the
    /// error message if failing to parse.
    fn str_then_parse<'l, T: FromStr>(&self, i: usize, in_line: Line<'l>) -> Result<T>
    where
        <T as FromStr>::Err: Send + Sync + Error + 'static,
    {
        Ok(self
            .get_str_then_parse(i, in_line)?
            .expect("expected capture to be present"))
    }
}
