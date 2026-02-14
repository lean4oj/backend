use core::{ffi::CStr, slice};
use std::io;

use tokio::io::{AsyncRead, AsyncReadExt};

#[path = "../libs/validate/lean.rs"]
mod validate;

use validate::is_lean_id;

const XATTR_NAME: &CStr = c"user.lean4oj.replay_flag";

pub async fn run<R>(dirname: &str, reader: &mut R) -> io::Result<(String, bool)>
where
    R: AsyncRead + Unpin,
{
    let len = reader.read_u32_le().await?;
    let tot = dirname.len() + len as usize;
    let mut buf = String::with_capacity(tot + 7);
    buf.push_str(dirname);
    reader.read_exact(unsafe { slice::from_raw_parts_mut(buf.as_mut_ptr().add(dirname.len()), len as usize) }).await?;
    unsafe { buf.as_mut_vec().set_len(tot); }
    buf.push_str(".olean\0");
    let part = unsafe { buf.get_unchecked(dirname.len()..tot) };
    if !part.split('/').all(is_lean_id) {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    cfg_select! {
        target_os = "linux" => {
            tracing::info!(target: "replay-memorizer", "check file {buf}");
            let res = unsafe { libc::getxattr(buf.as_ptr().cast(), XATTR_NAME.as_ptr(), core::ptr::null_mut(), 0) };
            if res >= 0 {
                Ok((buf, true))
            } else {
                let e = io::Error::last_os_error();
                if e.raw_os_error() == Some(libc::ENODATA) {
                    Ok((buf, false))
                } else {
                    Err(e)
                }
            }
        }
        _ => {
            Err(io::Error::from(io::ErrorKind::Unsupported))
        }
    }
}

#[cfg(target_os = "linux")]
pub fn mark(path_bank: Vec<String>) {
    for path in path_bank {
        unsafe {
            libc::setxattr(path.as_ptr().cast(), XATTR_NAME.as_ptr(), core::ptr::null(), 0, 0);
        }
    }
}
