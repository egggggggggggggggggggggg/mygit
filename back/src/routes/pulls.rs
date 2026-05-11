use std::sync::Arc;

use axum::extract::{Path, State};

use crate::{
    AppState,
    routes::{auth::MaybeAuthUser, issues::IssuePath},
};

pub async fn list_pulls(
    MaybeAuthUser(maybe_claims): MaybeAuthUser,
    Path(path): Path<IssuePath>,
    State(state): State<Arc<AppState>>,
) {
}
pub async fn create_pull() {}
