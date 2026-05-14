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
use axum::http::Method;
use back::app_routes::{
    auth::{__path_login, __path_logout, __path_refresh, __path_signup},
    comments::{__path_create_comment, __path_get_comments},
    issues::{__path_create_issue, __path_get_issue, __path_list_issues},
    pulls::{__path_close_pull, __path_list_pulls, __path_open_pull},
    repos::{
        __path_create_repo, __path_delete_repo, __path_list_commits, __path_repo_home,
        __path_repo_tree, __path_update_repo_metadata, __path_view_file,
    },
    storage::{__path_get_file, __path_upload},
    users::{__path_update_user, __path_user_profile},
};
use back::{
    AppState,
    app_routes::{
        auth::{login, logout, refresh, signup},
        comments::{create_comment, get_comments},
        issues::{create_issue, get_issue, list_issues},
        pulls::{close_pull, list_pulls, open_pull},
        repos::{
            create_repo, delete_repo, list_commits, repo_home, repo_tree, update_repo_metadata,
            view_file,
        },
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
use utoipa::openapi::security::{Http, HttpAuthScheme, SecurityScheme};
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_swagger_ui::SwaggerUi;
static JWT_SECRET: OnceLock<Vec<u8>> = OnceLock::new();

pub fn jwt_secret() -> &'static [u8] {
    JWT_SECRET.get_or_init(|| {
        std::env::var("JWT_SECRET")
            .expect("JWT_SECRET must be set")
            .into_bytes()
    })
}
///There should be rate limiting middleware and cors middleware, but I can't figure out how to
///properly do it. For some reason the type RateLimitLayer contains does not implement Clone when it
///itself implements Clone? Prevents me from using it. Might be doing something wrong.
#[tokio::main]
async fn main() {
    dotenv().ok();
    let host_address = std::env::var("SERVE_ADDRESS").unwrap();
    let git_storage = PathBuf::from(std::env::var("GIT_REPO_PATH").unwrap());
    let db_url = std::env::var("DATABASE_URL").unwrap();
    let file_storage = PathBuf::from(std::env::var("FILE_STORAGE").unwrap());
    let state = Arc::new(AppState {
        pool: PgPool::connect(&db_url).await.unwrap(),
        git_storage,
        file_storage,
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
    //the paths are most likely wrong here. gotta double check them. mixed a body extractor with
    //route extractor.
    let (router, mut openapi) = OpenApiRouter::new()
        .routes(routes!(signup))
        .routes(routes!(login))
        .routes(routes!(refresh))
        .routes(routes!(logout))
        .routes(routes!(upload))
        .routes(routes!(get_file))
        .routes(routes!(create_repo))
        .routes(routes!(delete_repo))
        .routes(routes!(list_commits))
        .routes(routes!(repo_home))
        .routes(routes!(repo_tree))
        .routes(routes!(update_repo_metadata))
        .routes(routes!(view_file))
        .routes(routes!(update_user))
        .routes(routes!(user_profile))
        .routes(routes!(list_issues))
        .routes(routes!(create_issue))
        .routes(routes!(get_issue))
        .routes(routes!(list_pulls))
        .routes(routes!(open_pull))
        .routes(routes!(close_pull))
        .routes(routes!(create_comment))
        .routes(routes!(get_comments))
        .with_state(state)
        .split_for_parts();
    openapi.components.as_mut().unwrap().add_security_scheme(
        "bearerAuth",
        SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
    );
    openapi.info.title = String::from("MyGit Backend");
    openapi.info.description = Some(String::from("Self hosted git service"));
    openapi.info.contact = None;
    openapi.info.version = String::from("");
    openapi.info.license = None;
    let app = router.merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", openapi.clone()));
    axum::serve(listener, app).await.unwrap()
}
#[cfg(test)]
mod tests {
    use back::app_routes::auth::{AuthResponse, LoginRequest, RefreshRequest, SignupRequest};
    use dotenvy::dotenv;
    use reqwest::StatusCode;
    use uuid::Uuid;

    const DEFAULT_BASE: &str = "http://127.0.0.1:3000";
    fn base_url() -> String {
        dotenv().ok();
        std::env::var("BASE_URL").unwrap_or_else(|_| DEFAULT_BASE.to_string())
    }

    #[tokio::test]
    async fn signup_login_refresh_logout_flow() {
        let client = reqwest::Client::new();
        let base = base_url();

        // Unique credentials per test run
        let uid = Uuid::new_v4().to_string();
        let email = format!("test+{}@example.com", uid);
        let username = format!("testuser_{}", uid);
        let password = "password123".to_string(); // meets >=8 requirement

        // 1) Signup
        let signup = SignupRequest {
            email: email.clone(),
            username,
            password: password.clone(),
        };
        let resp = client
            .post(format!("{}/auth/signup", base))
            .json(&signup)
            .send()
            .await
            .expect("signup request failed");
        assert_eq!(resp.status(), StatusCode::OK);
        let auth: AuthResponse = resp.json().await.expect("invalid signup response json");
        assert!(!auth.access_token.is_empty());
        assert!(!auth.refresh_token.is_empty());

        // 2) Use profile endpoint with access token (protected route)
        let profile_resp = client
            .get(format!("{}/users/profile", base))
            .bearer_auth(&auth.access_token)
            .send()
            .await
            .expect("profile request failed");
        assert_eq!(profile_resp.status(), StatusCode::OK);

        // 3) Login (using identifier = email)
        let login_req = LoginRequest {
            identifier: email,
            password,
        };
        let resp = client
            .post(format!("{}/auth/login", base))
            .json(&login_req)
            .send()
            .await
            .expect("login request failed");
        assert_eq!(resp.status(), StatusCode::OK);
        let login_auth: AuthResponse = resp.json().await.expect("invalid login response json");
        assert!(!login_auth.access_token.is_empty());
        assert!(!login_auth.refresh_token.is_empty());

        // 4) Refresh tokens
        let refresh_req = RefreshRequest {
            refresh_token: login_auth.refresh_token,
        };
        let resp = client
            .post(format!("{}/auth/refresh", base))
            .json(&refresh_req)
            .send()
            .await
            .expect("refresh request failed");
        assert_eq!(resp.status(), StatusCode::OK);
        let refreshed: AuthResponse = resp.json().await.expect("invalid refresh response json");
        assert!(!refreshed.access_token.is_empty());
        assert!(!refreshed.refresh_token.is_empty());

        // 5) Logout (invalidate refresh token)
        let logout_req = RefreshRequest {
            refresh_token: refreshed.refresh_token,
        };
        let resp = client
            .post(format!("{}/auth/logout", base))
            .json(&logout_req)
            .send()
            .await
            .expect("logout request failed");
        assert_eq!(resp.status(), StatusCode::OK);

        // 6) Refresh with same token should fail (401 or equivalent)
        let resp = client
            .post(format!("{}/auth/refresh", base))
            .json(&logout_req)
            .send()
            .await
            .expect("refresh-after-logout request failed");
        assert!(
            resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::BAD_REQUEST,
            "expected unauthorized/bad request after logout, got {}",
            resp.status()
        );
    }

    #[tokio::test]
    async fn login_with_wrong_password_fails() {
        let client = reqwest::Client::new();
        let base = base_url();

        // Create a user to test against
        let uid = Uuid::new_v4().to_string();
        let email = format!("test+{}@example.com", uid);
        let username = format!("testuser_{}", uid);
        let password = "password123".to_string();

        // Signup
        let signup = SignupRequest {
            email: email.clone(),
            username,
            password,
        };
        let resp = client
            .post(format!("{}/auth/signup", base))
            .json(&signup)
            .send()
            .await
            .expect("signup request failed");
        assert_eq!(resp.status(), StatusCode::OK);

        // Attempt login with wrong password
        let login_req = LoginRequest {
            identifier: email,
            password: "wrongpassword".to_string(),
        };
        let resp = client
            .post(format!("{}/auth/login", base))
            .json(&login_req)
            .send()
            .await
            .expect("login request failed");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
