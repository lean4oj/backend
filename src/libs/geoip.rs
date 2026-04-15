use core::{
    convert::Infallible,
    future::{Ready, ready},
    net::IpAddr,
};
use std::sync::OnceLock;

use axum::extract::FromRequestParts;
use http::request::Parts;
use maxminddb::{LookupResult, MaxMindDbError, PathElement, Reader};

use crate::libs::constants::REMOTE_ADDR;

static DB: OnceLock<Reader<Vec<u8>>> = OnceLock::new();

#[repr(transparent)]
pub struct Ip(pub Option<IpAddr>);

impl<S> FromRequestParts<S> for Ip {
    type Rejection = Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Ready<Result<Self, Self::Rejection>> {
        ready(Ok(Self(
            parts.headers.get(REMOTE_ADDR).and_then(
                |ip_str| IpAddr::parse_ascii(ip_str.as_ref()).ok()
            )
        )))
    }
}

fn query(ip: IpAddr) -> Result<LookupResult<'static, Vec<u8>>, MaxMindDbError> {
    {
        #[cfg(feature = "build-std")]
        unsafe { DB.get_unchecked() }
        #[cfg(not(feature = "build-std"))]
        unsafe { DB.get().unwrap_unchecked() }
    }.lookup(ip)
}

pub fn in_china(ip: IpAddr) -> bool {
    const CHINA: usize = 1_814_991;

    let Ok(res) = query(ip) else { return false };
    let country_id = res.decode_path(&[
        PathElement::Key("country"),
        PathElement::Key("geoname_id"),
    ]);
    matches!(country_id, Ok(Some(CHINA)))
}

pub fn init() {
    let db = Reader::open_readfile("GeoLite2-Country.mmdb").unwrap();
    tracing::info!("GeoIP database loaded: {:?}", db.metadata);
    DB.get_or_init(|| db);
}
