//Left a lot of notes around so it might look ugly.
//The errors in this are really janky, but this is just to get it up and running.
//Currently don't have any indexes, will add later if needed.
//To satisfy the file upload requirement, I'll probably do something like a banner/profile pic or
//smth.
use axum::{
    Router,
    http::Method,
    response::Html,
    routing::{get, post},
};
use back::{
    AppState, CacheLayer,
    routes::{
        auth::{login, refresh, signup},
        issues::{create_issue, get_issue, list_issues},
        pulls::{create_pull, list_pulls},
        repo::{create_repo, list_commits, repo_home, repo_tree, update_repo_metadata, view_file},
        users::user_profile,
    },
};
use dotenvy::dotenv;
use sqlx::PgPool;
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tower::limit::RateLimitLayer;
use tower_http::cors::{Any, CorsLayer};
static JWT_SECRET: OnceLock<Vec<u8>> = OnceLock::new();

pub fn jwt_secret() -> &'static [u8] {
    JWT_SECRET.get_or_init(|| {
        std::env::var("JWT_SECRET")
            .expect("JWT_SECRET must be set")
            .into_bytes()
    })
}
#[tokio::main]
async fn main() {
    dotenv().ok();
    let host_address = std::env::var("SERVE_ADDRESS").unwrap();
    let git_storage = PathBuf::from(std::env::var("GIT_REPO_PATH").unwrap());
    let db_url = std::env::var("DATABASE_URL").unwrap();
    let state = Arc::new(AppState {
        pool: PgPool::connect(&db_url).await.unwrap(),
        git_storage,
        cache: CacheLayer::default(),
        jwt_secret: jwt_secret(),
    });
    let listener = tokio::net::TcpListener::bind(host_address.clone())
        .await
        .unwrap();
    let _cors_layer =
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([Method::GET, Method::PUT, Method::POST]);
    let _rate_limit_layer = RateLimitLayer::new(10, Duration::from_secs(1));
    println!("Listening on {}", host_address);
    //Serve
    let app = Router::new()
        .route("/", get(handler))
        .route("/refresh", get(refresh))
        .route("/login", get(login))
        .route("/signup", get(signup))
        // user
        .route("/{username}", get(user_profile))
        // repos
        .route("/repos", post(create_repo))
        .route(
            "/{username}/{repo}",
            get(repo_home).patch(update_repo_metadata),
        )
        .route("/{username}/{repo}/commits", get(list_commits))
        // issues
        .route(
            "/{username}/{repo}/issues",
            get(list_issues).post(create_issue),
        )
        .route("/{username}/{repo}/issues/{id}", get(get_issue))
        // pulls
        .route(
            "/{username}/{repo}/pulls",
            get(list_pulls).post(create_pull),
        )
        // browsing
        .route("/{username}/{repo}/tree/{branch}/{*path}", get(repo_tree))
        .route("/{username}/{repo}/blob/{branch}/{*path}", get(view_file))
        .with_state(state);
    axum::serve(listener, app).await.unwrap();
}
async fn handler() -> Html<&'static str> {
    Html("<h1>Placeholder</h1>")
}
