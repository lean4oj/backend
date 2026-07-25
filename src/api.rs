use axum::{Router, routing::post_service};
use http::{HeaderValue, Response, header};
use tower_http::cors::CorsLayer;

use crate::libs::{
    constants::{APPLICATION_JSON_UTF_8, X_ACCEL_REDIRECT},
    request::RawPayload,
};

mod auth;
mod discussion;
pub mod fs;
mod group;
mod homepage;
mod judge_client;
mod leansearch;
mod problem;
mod proof_of_work;
mod submission;
mod user;

fn plausible() -> RawPayload {
    let mut parts = Response::new(()).into_parts().0;
    parts.headers.insert(X_ACCEL_REDIRECT, const { HeaderValue::from_static("@plausible") });
    RawPayload { header: Box::leak(Box::new(parts)), body: &[] }
}

pub fn all() -> Router {
    let cors = CorsLayer::very_permissive().allow_private_network(true);

    let mut parts = Response::new(()).into_parts().0;
    parts.headers.insert(header::CONTENT_TYPE, APPLICATION_JSON_UTF_8);
    let header = Box::leak(Box::new(parts));

    Router::new()
        .nest("/auth", auth::router(header))
        .nest("/discussion", discussion::router(header))
        .route("/event", post_service(plausible()))
        .nest("/group", group::router(header))
        .nest("/homepage", homepage::router(header))
        .nest("/judgeClient", judge_client::router(header))
        .nest("/leansearch", leansearch::router(header))
        .nest("/problem", problem::router(header))
        .nest("/proofOfWork", proof_of_work::router(header))
        .nest("/submission", submission::router(header))
        .nest("/user", user::router(header))
        .layer(cors)
}
