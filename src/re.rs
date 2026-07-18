/// Expands to code that instantiates a `regex::Regex` instance on
/// first access, but caches it for the remainder of the life of the
/// program (i.e. returns `&'static Regex`). Note that this panics for
/// invalid regular expressions when accessed.
#[macro_export]
macro_rules! re {
    { $regex_string:expr } => {
        {
            ::lazy_static::lazy_static!{
                static ref RE: ::regex::Regex = ::regex::Regex::new($regex_string).unwrap();
            }
            &*RE
        }
    }
}
