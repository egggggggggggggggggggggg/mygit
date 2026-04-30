use std::env;

use axum::Router;
use dotenvy::dotenv;

#[tokio::main]
async fn main() {
    dotenv().ok();
    let git_path = env::var("PATH");
}
