use core::{convert::Infallible, mem, ptr};
use std::{fs, sync::OnceLock, time::SystemTime};

use axum::extract::FromRequestParts;
use base64::{Engine, prelude::{BASE64_STANDARD, BASE64_STANDARD_NO_PAD}};
use futures_util::{FutureExt, future::Map};
use http::{header::AUTHORIZATION, request::Parts};
use openssl::{bn::BigNum, ec::EcKey, ecdsa::EcdsaSig, pkey::Private};
use tower_sessions_core::{Session, session::Id};

use crate::models::user::User;

use super::{
    db::{PooledConnection, insert_connection},
    session::{self, GlobalStore},
};

pub mod availability;
mod email_verification;
pub use email_verification::{
    CodeType as EmailVerificationCodeType, delete_expired, email_check, get_code, get_email_content,
};

pub enum Session_ {
    None,
    Session(Session<GlobalStore>),
    Token(User),
}

impl<S: Sync> FromRequestParts<S> for Session_ {
    type Rejection = Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Map<impl Future<Output = Self>, fn(Self) -> Result<Self, Infallible>> {
        decode(parts, parts.extensions.get().copied()).map(Ok)
    }
}

async fn decode(parts: &Parts, now: Option<SystemTime>) -> Session_ {
    if let Some(now) = now
    && let Some(header) = parts.headers.get(AUTHORIZATION)
    && let Some(base64) = header.as_bytes().strip_prefix(b"Bearer ") {
        decode2(base64, now, &mut None).await
    } else {
        Session_::None
    }
}

pub async fn decode2(raw: &[u8], now: SystemTime, db: &mut Option<PooledConnection>) -> Session_ {
    const SQL: &str = "update lean4oj.user_api_tokens set last_used_at = $1 from lean4oj.users where user_api_tokens.uid = users.uid and token = $2 returning users.uid, username, email, password, register_time, ac, nickname, bio, avatar_info";
    if let Some(token) = raw.strip_prefix(b"l4oj-") {
        let mut buf = [0u8; 64];
        if BASE64_STANDARD_NO_PAD.decode_slice(token, &mut buf) == Ok(64) {
            let Ok(db) = insert_connection(db).await else { return Session_::None };
            let res = try {
                let stmt = db.prepare_static(SQL.into()).await?;
                let row = db.query_one(&stmt, &[&now, &buf.as_slice()]).await?;
                User::try_from(row)?
            };
            match res {
                Ok(user) => Session_::Token(user),
                Err(_) => Session_::None,
            }
        } else {
            Session_::None
        }
    } else {
        let Ok(encoded) = Encoded::try_from(raw) else { return Session_::None };
        if encoded.verify() && let Ok(session) = session::load(encoded.id).await {
            Session_::Session(session)
        } else {
            Session_::None
        }
    }
}

#[repr(C)]
pub struct Encoded {
    pub id: Id,
    r: [u8; 48],
    s: [u8; 48],
}

impl Encoded {
    pub fn verify(&self) -> bool {
        let Ok(r) = BigNum::from_slice(&self.r) else { return false };
        let Ok(s) = BigNum::from_slice(&self.s) else { return false };
        let Ok(sign) = EcdsaSig::from_private_components(r, s) else { return false };
        matches!(
            sign.verify(
                &self.id.0.to_be_bytes(),
                #[cfg(feature = "build-std")]
                unsafe { ECKEY.get_unchecked() },
                #[cfg(not(feature = "build-std"))]
                unsafe { ECKEY.get().unwrap_unchecked() },
            ),
            Ok(true),
        )
    }
}

impl TryFrom<Id> for Encoded {
    type Error = openssl::error::ErrorStack;

    fn try_from(id: Id) -> Result<Self, Self::Error> {
        let sign = EcdsaSig::sign(
            &id.0.to_be_bytes(),
            #[cfg(feature = "build-std")]
            unsafe { ECKEY.get_unchecked() },
            #[cfg(not(feature = "build-std"))]
            unsafe { ECKEY.get().unwrap_unchecked() },
        )?;
        let raw_sign: *const openssl_sys::ECDSA_SIG = unsafe { mem::transmute_copy(&sign) };
        let mut r0 = ptr::null();
        let mut s0 = ptr::null();
        let mut r = [0u8; 48];
        let mut s = [0u8; 48];
        unsafe {
            openssl_sys::ECDSA_SIG_get0(raw_sign, &raw mut r0, &raw mut s0);
            openssl_sys::BN_bn2binpad(r0, r.as_mut_ptr(), 48);
            openssl_sys::BN_bn2binpad(s0, s.as_mut_ptr(), 48);
        }
        Ok(Self { id, r, s })
    }
}

impl TryFrom<&[u8]> for Encoded {
    type Error = ();

    fn try_from(src: &[u8]) -> Result<Self, Self::Error> {
        const N: usize = mem::size_of::<Encoded>();
        let mut buf = [0u8; N];
        if BASE64_STANDARD.decode_slice(src, &mut buf) == Ok(N) {
            Ok(unsafe { mem::transmute::<[u8; N], Self>(buf) })
        } else {
            Err(())
        }
    }
}

static ECKEY: OnceLock<EcKey<Private>> = OnceLock::new();

pub fn init() {
    const PRIVATE_KEY_PATH: &str = "/usr/local/nginx/conf/private.key";

    let key_pem = fs::read(PRIVATE_KEY_PATH).unwrap();
    ECKEY.get_or_init(|| EcKey::private_key_from_pem(&key_pem).unwrap());
}
