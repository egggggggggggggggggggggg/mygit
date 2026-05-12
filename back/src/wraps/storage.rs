use axum::{
    Json,
    extract::{Multipart, State},
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::{fs, fs::File, io::AsyncWriteExt};
use uuid::Uuid;

use crate::{AppState, routes::auth::AuthUser};

#[axum::debug_handler]
pub async fn upload(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<Vec<String>>, String> {
    let upload_root = state.file_storage.clone();

    let mut stored_paths = Vec::new();

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
        sqlx::query!(
            r#"
            INSERT INTO files (
                storage_key,
                original_filename,
                mime_type,
                size_bytes, uploader_id
            )
            VALUES ($1, $2, $3, $4, $5)
            "#,
            storage_key,
            original_filename,
            mime_type,
            size_bytes,
            claims.sub,
        )
        .execute(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
        stored_paths.push(storage_key);
    }
    Ok(Json(stored_paths))
}
//General process, user uploads file and frontend stages  it or smth.
//In the meantime the frontend attempts to send the backend the image and have it be stored. Once
//the backend successfully stored the image we can then set the associated file wit the associated
//comment within the comment_files join table. next time we load the comment we can just consult
//the comment_file join table for the file.
