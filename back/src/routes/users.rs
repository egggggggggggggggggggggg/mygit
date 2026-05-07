use std::sync::Arc;

use axum::extract::{Path, State};

use crate::AppState;

pub async fn user_profile(State(t): State<Arc<AppState>>, Path(params): Path<String>) {}

pub struct UpdateRequest {}
pub async fn update_user() {}
