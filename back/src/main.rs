use axum::{Extension, Router, response::Html, routing::get};
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
use std::{path::PathBuf, sync::Arc};

#[tokio::main]
async fn main() {
    dotenv().ok();
    let host_address = std::env::var("SERVE_ADDRESS").unwrap();
    let path = PathBuf::from(std::env::var("GIT_REPO_PATH").unwrap());
    let db_url = std::env::var("DATABASE_URL").unwrap();
    ///This is wrong, it shouldn't be trying to find a single repo, rather a folder of bare repos.
    ///Each bare repo correlates to an actual user repo.  
    let repo = match gix::discover(path.clone()) {
        Ok(repo) => repo,
        Err(e) => panic!("Invalid repo: {}", e),
    };
    let state = Arc::new(AppState {
        pool: PgPool::connect(&db_url).await.unwrap(),
    });
    if !repo.is_bare() {
        panic!("Not a bare repo: {:?}", path);
    }
    let listener = tokio::net::TcpListener::bind(host_address).await.unwrap();
    println!("Listening on {}", host_address);
    let app = Router::new()
        .route("/", get(handler))
        .route("/login", get(login))
        .route("/signup", get(signup))
        .route("/:username", get(user_profile))
        .route(
            "/:username/:repo",
            get(repo_home).post(create_repo).put(update_repo),
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
