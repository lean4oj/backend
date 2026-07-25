use axum::{Router, routing::post_service};
use http::response::Parts;

use crate::libs::request::RawPayload;

const fn issue_challenge(header: &'static Parts) -> RawPayload {
    RawPayload { header, body: br#"{"id":"","randomData":"","difficulty":0,"expiresAt":9007199254740991}"# }
}

pub fn router(header: &'static Parts) -> Router {
    Router::new()
        .route("/issueChallenge", post_service(issue_challenge(header)))
}
