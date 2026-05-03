use axum::{Router, routing::post};
use http::response::Parts;

async fn search() {}

pub fn router(header: &'static Parts) -> Router {
    Router::new().route("/search", post(search))
}
