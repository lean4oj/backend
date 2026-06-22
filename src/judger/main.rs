use core::slice;
use std::{borrow::Cow, io, process::Stdio};

use http::{Request, header};
use http_body_util::BodyExt;
use hyper::{
    client::conn::{self, http1::SendRequest},
    rt::{Read, Write},
};
use nodejs_semver::{Identifier, Version};
use serde::Serialize;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};

use crate::{
    constants::{APPLICATION_JSON_UTF_8, DUMMY_HOST, PASSWORD, USERNAME},
    task, verified,
};

#[path = "../models/submission/message.rs"]
mod message;
#[path = "../models/submission/status.rs"]
mod status;

#[derive(Serialize)]
struct Report<'a> {
    uid: &'a str,
    password: &'a str,
    sid: u32,
    status: status::Status,
    message: message::Action,
    answer: Option<&'a str>,
}

async fn report(
    sid: u32,
    status: status::Status,
    message: message::Action,
    answer: Option<&str>,
    sender: &mut SendRequest<String>,
) -> io::Result<()> {
    #[cfg(debug_assertions)]
    tracing::debug!("[submission #{sid}] status = {status:?}, message = {message:?}, answer = {answer:?}");

    let s = Report {
        uid: USERNAME,
        password: PASSWORD,
        sid,
        status,
        message,
        answer,
    };
    let req = Request::post("/api/submission/judger__report__status")
        .header(header::HOST, DUMMY_HOST)
        .header(header::CONTENT_TYPE, APPLICATION_JSON_UTF_8)
        .body(serde_json::to_string(&s)?)
        .unwrap();

    sender.ready().await.map_err(io::Error::other)?;
    let res = sender.try_send_request(req).await
        .map_err(|e| io::Error::other(e.into_error()))?;

    match res.into_body().collect().await {
        Ok(_) => Ok(()),
        Err(e) => Err(io::Error::other(e)),
    }
}

async fn read_string<R>(reader: &mut R) -> io::Result<String>
where
    R: AsyncRead + Unpin,
{
    let len = reader.read_u32_le().await?;
    let mut buf = String::with_capacity(len as usize);
    reader.read_exact(unsafe { slice::from_raw_parts_mut(buf.as_mut_ptr(), len as usize) }).await?;
    unsafe { buf.as_mut_vec().set_len(len as usize); }
    Ok(buf)
}

#[cfg(target_os = "linux")]
fn set_nice() -> io::Result<()> {
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 19);
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
pub async fn main_loop<S>(sock: S) -> hyper::Result<()>
where
    S: Read + Write + Send + Unpin + 'static,
{
    #[inline]
    fn is_legacy(version: &str) -> bool {
        let Ok(version) = Version::parse(version) else { return false };
        match core::intrinsics::three_way_compare(version.minor(), 30) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Equal => version.patch() == 0 && matches!(*version.pre_release(), [Identifier::AlphaNumeric(deref!("rc1"))]),
            std::cmp::Ordering::Greater => false,
        }
    }

    let (mut sender, conn) = conn::http1::handshake::<_, String>(sock).await?;
    let conn_backend = tokio::spawn(conn.with_upgrades());

    while !conn_backend.is_finished() {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let task = match task::get(&mut sender).await {
            Ok(Ok(t)) => t,
            Ok(Err(e)) => { tracing::warn!("Failed to deserialize task: {e}"); continue; }
            Err(e) => { tracing::warn!("Failed to get task: {e}"); continue; }
        };

        #[cfg(debug_assertions)]
        tracing::debug!("Received task: {task:?}");

        let bytes = task.sid.to_le_bytes();
        let version_placeholder = if is_legacy(&task.version) {
            ""
        } else {
            "/.lake/build/lib/lean"
        };
        let lean_path = format!(
            "{0}/leanprover--lean4---v{2}/lib/lean:{1}/std/{2}:{1}/lean/Lean4OJ/{2}{3}:{1}/submissions/{7:02x}/{6:02x}/{5:02x}/{4:02x}/main.lean",
            env!("LEAN4_TOOLCHAIN_DIR"),
            env!("OLEAN_ROOT"),
            task.version,
            version_placeholder,
            bytes[0], bytes[1], bytes[2], bytes[3],
        );
        let arg = unsafe { lean_path.get_unchecked(lean_path.len() - const { env!("OLEAN_ROOT").len() + 34 }..) };
        let dirname_arg = unsafe { arg.get_unchecked(..arg.len() - 9) };

        let mut cmd = Command::new(format!("l4judger-{}", task.version));
        cmd.env("LEAN_PATH", unsafe { lean_path.get_unchecked(..lean_path.len() - 10) });
        cmd.arg(arg);
        cmd.args(task.axioms);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt;

            cmd.uid(0x10000 + task.sid);
            cmd.gid(0xdeadbeef);
            cmd.as_std_mut().groups(&[]);
            unsafe { cmd.pre_exec(set_nice); }
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to spawn l4judger: {e}");
                let _ = report(task.sid, status::Status::JudgementFailed, message::Action::Replace(Cow::Owned(e.to_string())), None, &mut sender).await;
                continue;
            }
        };
        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = child.stdout.take().unwrap();

        #[cfg(target_os = "linux")]
        let (mut has_ac, mut path_bank) = (false, hashbrown::HashSet::new());
        // main loop
        while let Ok(status_raw) = stdout.read_u8().await {
            if let Ok(status) = status::Status::try_from(status_raw)
            && let Ok(message_raw) = stdout.read_u8().await {
                let message = match message_raw {
                    0 => message::Action::NoAction,
                    1 => {
                        let Ok(s) = read_string(&mut stdout).await else { break };
                        message::Action::Replace(Cow::Owned(s))
                    }
                    2 => {
                        let Ok(s) = read_string(&mut stdout).await else { break };
                        message::Action::Append(Cow::Owned(s))
                    }
                    _ => break,
                };
                let Ok(has_answer) = stdout.read_u8().await else { break };
                let answer = match has_answer {
                    0 => None,
                    1 => {
                        let Ok(s) = read_string(&mut stdout).await else { break };
                        Some(s)
                    }
                    _ => break,
                };
                #[cfg(target_os = "linux")]
                (has_ac = has_ac || status == status::Status::Accepted);
                if let Err(e) = report(task.sid, status, message, answer.as_deref(), &mut sender).await {
                    tracing::warn!("Failed to report status: {e:?}");
                }
            } else {
                #[allow(clippy::single_match, unused_variables, reason = "for future extension")]
                match status_raw {
                    128 => {
                        let ret = if let Ok((path, ret)) = verified::run(dirname_arg, &mut stdout).await {
                            #[cfg(target_os = "linux")]
                            path_bank.insert(path);
                            u8::from(ret)
                        } else {
                            0
                        };
                        if stdin.write_u8(ret).await.is_err() { break; }
                    }
                    _ => (),
                }
            }
        }

        let err = match child.wait().await {
            Ok(status) => match status.exit_ok() {
                Ok(()) => None,
                Err(e) => Some(e.to_string())
            },
            Err(e) => Some(e.to_string()),
        };

        if let Some(err) = err {
            tracing::warn!("l4judger process failed: {err}");
            let _ = report(task.sid, status::Status::JudgementFailed, message::Action::Replace(Cow::Owned(err)), None, &mut sender).await;
        }

        #[cfg(target_os = "linux")]
        if has_ac {
            tokio::task::spawn_blocking(|| verified::mark(path_bank));
        }
    }

    conn_backend.await.unwrap()
}
