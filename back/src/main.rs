//Left a lot of notes around so it might look ugly.
//The errors in this are really janky, but this is just to get it up and running.
//Currently don't have any indexes, will add later if needed.
//To satisfy the file upload requirement, I'll probably do something like a banner/profile pic or
//smth.
//A lot of redundant extractor ccode logic which involves verifying that the user can actually view
//the repo. Could have it built into the extractor but currently the isuse is making multiple queries.
//Trying to reduce queries to a single one per function. Might be worse in the long run as it hides
//errors behind a database error making it more annoying to debug.
//A lot of these functions have multiple queries. If imma continue working on this I'll probably
//try and optimize some of them into a single one. For the tables I threw uuid everywhere when
//BIGSERIAL/SERIAL could've been fine.
use axum::{
    Router,
    http::Method,
    routing::{get, post},
};
use back::{
    AppState, CacheLayer,
    routes::{
        auth::{login, logout, refresh, signup},
        issues::{create_issue, get_issue, list_issues},
        pulls::{list_pulls, open_pull},
        repo::{create_repo, list_commits, repo_home, repo_tree, update_repo_metadata, view_file},
        storage::{get_file, upload},
        users::{update_user, user_profile},
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
        //Placeholder
        file_storage: PathBuf::new(),
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
        // auth
        .route("/auth/signup", post(signup))
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh))
        .route("/auth/logout", post(logout))
        //user should first upload the files before doing anything regarding a comment.
        .route("/upload", post(upload))
        .route("/files/{id}", get(get_file))
        .route("/users/{username}", get(user_profile).patch(update_user))
        // repositories
        .route("/user/repos", post(create_repo))
        .route(
            "/{username}/{repo}",
            get(repo_home).patch(update_repo_metadata),
        )
        // commits
        .route("/{username}/{repo}/commits", get(list_commits))
        // issues
        .route(
            "/{username}/{repo}/issues",
            get(list_issues).post(create_issue),
        )
        .route("/{username}/{repo}/issues/{id}", get(get_issue))
        // pulls
        .route("/{username}/{repo}/pulls", get(list_pulls).post(open_pull))
        // git browsing
        .route("/{username}/{repo}/tree/{branch}/{*path}", get(repo_tree))
        .route("/{username}/{repo}/blob/{branch}/{*path}", get(view_file))
        .with_state(state);
    axum::serve(listener, app).await.unwrap()
}
