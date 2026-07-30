use std::io::{Error, Write};

use bstr::BString;

pub trait WriteTo {
    fn write_to(&self, out: impl Write) -> Result<(), Error>;

    fn to_bstring(&self) -> BString {
        let mut out = BString::new(Vec::new());
        self.write_to(&mut *out)
            .expect("writing to Vec does not fail");
        out
    }
}
