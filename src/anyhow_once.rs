use std::sync::Mutex;

use anyhow::anyhow;

/// Make an anyhow error takeable once; subsequent takes yield a
/// generic error
///
/// This is a work-around for the issue with lazy wrappers that errors
/// must be stored, but anyhow::Error does not implement Clone, and
/// Arc<anyhow::Error> is not anyhow-compatible (i.e. cannot be used
/// with .context)
pub struct AnyhowOnce(Mutex<Option<anyhow::Error>>);

impl From<anyhow::Error> for AnyhowOnce {
    fn from(value: anyhow::Error) -> Self {
        Self(Mutex::new(Some(value)))
    }
}

impl AnyhowOnce {
    pub fn take(&self) -> anyhow::Error {
        let mut guard = self.0.lock().expect("no panics");
        if let Some(e) = guard.take() {
            e
        } else {
            anyhow!("error has already been taken out")
        }
    }
}
