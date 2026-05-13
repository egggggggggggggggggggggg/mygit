use axum::{
    Json,
    body::Body,
    extract::{Multipart, Query, State},
    http::{HeaderValue, Response},
    response::IntoResponse,
};
use reqwest::header;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::{fs, fs::File, io::AsyncWriteExt};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::{
    AppState,
    errors::ApiError,
    routes::auth::{AuthUser, MaybeAuthUser},
};

#[axum::debug_handler]
pub async fn upload(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<Vec<Uuid>>, String> {
    let upload_root = state.file_storage.clone();
    let mut stored_file_ids = Vec::new();
    while let Some(mut field) = multipart.next_field().await.map_err(|e| e.to_string())? {
        // Read + hash into a temporary file first
        let mut size_bytes: i64 = 0;
        let temp_name = format!("tmp-{}", Uuid::new_v4());
        let temp_path = upload_root.join(temp_name);
        let mut temp_file = File::create(&temp_path).await.map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        while let Some(chunk) = field.chunk().await.map_err(|e| e.to_string())? {
            size_bytes += chunk.len() as i64;
            hasher.update(&chunk);
            temp_file
                .write_all(&chunk)
                .await
                .map_err(|e| e.to_string())?;
        }
        temp_file.flush().await.map_err(|e| e.to_string())?;
        drop(temp_file);
        let hash = hasher.finalize();
        let original_filename = field.file_name().unwrap_or("unknown").to_string();
        let mime_type = field
            .content_type()
            .map(|m| m.to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let hash_hex = hex::encode(hash);

        let dir1 = &hash_hex[0..2];
        let dir2 = &hash_hex[2..4];

        let final_dir = upload_root.join(dir1).join(dir2);

        fs::create_dir_all(&final_dir)
            .await
            .map_err(|e| e.to_string())?;
        let final_path = final_dir.join(&hash_hex);
        fs::rename(&temp_path, &final_path)
            .await
            .map_err(|e| e.to_string())?;
        let storage_key = format!("{}/{}/{}", dir1, dir2, hash_hex);
        //Change this to be atomic so we don't have orphan files when the db operation fails.
        //If insertion fails delete the file or smth.
        let file_id = sqlx::query!(
            r#"
            INSERT INTO files (
                storage_key,
                original_filename,
                mime_type,
                size_bytes,
                uploader_id
            )
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#,
            storage_key,
            original_filename,
            mime_type,
            size_bytes,
            claims.sub,
        )
        .fetch_one(&state.pool)
        .await
        .map_err(|e| e.to_string())?
        .id;
        stored_file_ids.push(file_id);
    }
    Ok(Json(stored_file_ids))
}

//The file cannot be orphaned. if it orphaned then the db should remove it.
pub async fn get_file(
    MaybeAuthUser(maybe_claims): MaybeAuthUser,
    State(state): State<Arc<AppState>>,
    Query(id): Query<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = maybe_claims.map(|c| c.sub);
    let can_access = sqlx::query_scalar!(
        r#"
        SELECT EXISTS ( 
            SELECT 1
            FROM files f
            JOIN comment_files cf ON cf.file_id = f.id
            JOIN comments c ON c.id = cf.comment_id
            JOIN repositories r ON r.id = c.repository_id
            WHERE f.id = $1
                AND (
                    r.is_private = false
                    OR r.owner_id = $2
                    OR EXISTS (
                        SELECT 1 
                        FROM repository_collaborators rc 
                        WHERE rc.repository_id = r.id 
                            AND rc.user_id = $2
                    )
            )
        )"#,
        id,
        user_id,
    )
    .fetch_one(&state.pool)
    .await?
    .unwrap_or(false);
    if !can_access {
        return Err(ApiError::Unauthorized);
    }
    let file = sqlx::query!(
        r#"
        SELECT
            storage_key,
            original_filename,
            mime_type
        FROM files
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::RepoNotFound)?;
    let path = state.file_storage.join(&file.storage_key);
    let disk_file = File::open(path).await?;
    let stream = ReaderStream::new(disk_file);
    let body = Body::from_stream(stream);
    let mut response = Response::new(body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&file.mime_type)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("inline; filename=\"{}\"", file.original_filename)).unwrap(),
    );
    //User has access to the resource. Start sending it over via multipart or octet stream.
    //Will implement this later.
    Ok(response)
}
//Check for orhpaned files that don't have any
pub async fn clean_files() {}

//General process, user uploads file and frontend stages  it or smth.
//In the meantime the frontend attempts to send the backend the image and have it be stored. Once
//the backend successfully stored the image we can then set the associated file wit the associated
//comment within the comment_files join table. next time we load the comment we can just consult
//the comment_file join table for the file.
