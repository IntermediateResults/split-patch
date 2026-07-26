use std::io::{Error, Write};

use bstr::BString;

pub trait WriteTo {
    type Owned: From<BString>;

    fn write_to(&self, out: impl Write) -> Result<(), Error>;

    fn to_bstring(&self) -> BString {
        let mut out = BString::new(Vec::new());
        self.write_to(&mut *out)
            .expect("writing to Vec does not fail");
        out
    }

    fn make_owned(&self) -> Self::Owned {
        self.to_bstring().into()
    }
}
