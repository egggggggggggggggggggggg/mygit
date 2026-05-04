use axum::{Router, response::Html, routing::get};
use back::{
    AppState,
    routes::{
        auth::{login, signup},
        issues::{create_issue, get_issue, list_issues, list_pulls, repo_tree, view_file},
        repo::{create_repo, repo_home, update_repo},
        users::user_profile,
    },
};
use dotenvy::dotenv;
use sqlx::PgPool;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tower::limit::RateLimitLayer;
#[tokio::main]
async fn main() {
    dotenv().ok();
    let host_address = std::env::var("SERVE_ADDRESS").unwrap();
    let git_storage = PathBuf::from(std::env::var("GIT_REPO_PATH").unwrap());
    let db_url = std::env::var("DATABASE_URL").unwrap();
    let state = Arc::new(AppState {
        pool: PgPool::connect(&db_url).await.unwrap(),
        git_storage,
    });
    let listener = tokio::net::TcpListener::bind(host_address.clone())
        .await
        .unwrap();
    println!("Listening on {}", host_address);
    let app = Router::new()
        .route("/", get(handler))
        .route(
            "/login",
            get(login).layer(RateLimitLayer::new(10, Duration::new(1, 0))),
        )
        .route("/signup", get(signup))
        .route("/:username", get(user_profile))
        .route(
            "/:username/:repo",
            get(repo_home).put(update_repo).post(create_repo),
        )
        .route(
            "/:username/:repo/issues",
            get(list_issues).post(create_issue),
        )
        .route("/:username/:repo/issues/:id", get(get_issue))
        .route("/:username/:repo/pulls", get(list_pulls))
        .route("/:username/:repo/tree/:branch/*path", get(repo_tree))
        .route("/:username/:repo/blob/:branch/*path", get(view_file))
        .with_state(state);
    let _ = axum::serve(listener, app).await;
}
async fn handler() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}
