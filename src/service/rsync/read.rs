use core::fmt;

use tokio::{
    io::{BufReader, BufWriter},
    net::unix::{OwnedReadHalf, OwnedWriteHalf},
};

use crate::libs::error::{BoxedStdError, DynStdError};

#[derive(Debug)]
struct UnsupportedReadError {
    sni: String,
    uid: String,
}

impl fmt::Display for UnsupportedReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Please use `wget -e robots=off -r -l 0 -nH -np --cut-dirs=1 -R 'index.html,index.html.tmp' '{}/lean/{}/'` (no backticks) to download files.", self.sni, self.uid)
    }
}

impl core::error::Error for UnsupportedReadError {
    fn source(&self) -> Option<&DynStdError> {
        Some(const { &core::io::Error::UNSUPPORTED_PLATFORM })
    }
}

pub fn main(
    _c2s: BufReader<OwnedReadHalf>,
    _s2c: BufWriter<OwnedWriteHalf>,
    sni: String,
    uid: String,
) -> Result<(), BoxedStdError> {
    Err(UnsupportedReadError { sni, uid }.into())
}
