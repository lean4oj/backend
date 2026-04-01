use std::sync::OnceLock;

use lettre::{
    Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::Mailbox,
    transport::smtp::{PoolConfig, authentication::Credentials, response::Response},
};

use super::constants::GLOBAL_INTERVAL;

static SOURCE: OnceLock<Mailbox> = OnceLock::new();
static MAILER: OnceLock<AsyncSmtpTransport<Tokio1Executor>> = OnceLock::new();

#[allow(clippy::unwrap_used)]
pub fn init_email() {
    const ADDR: &str = env!("LEAN4OJ_EMAIL_ADDRESS");
    const AT_START: usize = {
        let mut i = ADDR.len();
        loop {
            i -= 1;
            if ADDR.as_bytes()[i] == b'@' {
                break i;
            }
        }
    };

    let source = Mailbox::new(
        Some("Lean4OJ noreply".to_owned()),
        unsafe { Address::new_unchecked(ADDR.to_owned(), AT_START) },
    );
    let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay("smtp.163.com")
        .unwrap()
        .credentials(Credentials::new(
            unsafe { ADDR.get_unchecked(..AT_START) }.to_owned(),
            env!("LEAN4OJ_EMAIL_PASSWORD").to_owned(),
        ))
        .pool_config(PoolConfig::new().idle_timeout(GLOBAL_INTERVAL))
        .build::<Tokio1Executor>();

    tracing::info!(target: "mailer", ?source, ?mailer);

    SOURCE.get_or_init(|| source);
    MAILER.get_or_init(|| mailer);
}

#[inline(always)]
pub fn get_source() -> Mailbox {
    {
        #[cfg(feature = "build-std")]
        unsafe { SOURCE.get_unchecked() }
        #[cfg(not(feature = "build-std"))]
        unsafe { SOURCE.get().unwrap_unchecked() }
    }.clone()
}

#[inline(always)]
pub fn send_mail(
    message: Message,
) -> impl Future<Output = Result<Response, lettre::transport::smtp::Error>> {
    {
        #[cfg(feature = "build-std")]
        unsafe { MAILER.get_unchecked() }
        #[cfg(not(feature = "build-std"))]
        unsafe { MAILER.get().unwrap_unchecked() }
    }.send(message)
}
