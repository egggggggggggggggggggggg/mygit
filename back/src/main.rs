use axum::{Router, routing::get};
use back::{
    AppState,
    routes::{
        auth::{login, signup},
        issues::{create_issue, get_issue, list_issues, list_pulls, repo_tree, view_file},
        repo::repo_home,
        users::user_profile,
    },
};
use dotenvy::dotenv;
use sqlx::{Sqlite, SqlitePool};
use std::{io, path::PathBuf};
#[tokio::main]
async fn main() -> io::Result<()> {
    dotenv().ok();
    let path = PathBuf::from(std::env::var("GIT_REPO_PATH").unwrap());
    let db_url = std::env::var("DB_URL").unwrap();
    let repo = match gix::discover(path.clone()) {
        Ok(repo) => repo,
        Err(e) => panic!("Invalid repo: {}", e),
    };
    let state = AppState {
        pool: SqlitePool::connect(&db_url).await?,
    };
    if !repo.is_bare() {
        panic!("Not a bare repo: {:?}", path);
    }
    let app = Router::new()
        .route("/login", get(login))
        //Create a user for access to the server
        .route("/signup", get(signup))
        //Routes to handlers for a user.
        .with_state(state)
        .nest("/:username", user_routes());
    axum::serve(listener, app).await?;
    Ok(())
}
fn user_routes() -> Router {
    Router::new()
        .route("/", get(user_profile))
        .nest("/:repo", repo_routes())
}
fn repo_routes() -> Router {
    Router::new()
        .route("/", get(repo_home))
        .route("/issues", get(list_issues).post(create_issue))
        .route("/issues/:id", get(get_issue))
        .route("/pulls", get(list_pulls))
        .route("/tree/:branch/*path", get(repo_tree))
        .route("/blob/:branch/*path", get(view_file))
}
