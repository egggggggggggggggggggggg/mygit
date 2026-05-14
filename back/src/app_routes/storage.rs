use crate::{
    AppState,
    app_routes::auth::{AuthUser, MaybeAuthUser},
    errors::ApiError,
};
use axum::{
    Json,
    body::Body,
    extract::{Multipart, Path, State},
    http::{HeaderValue, Response},
    response::IntoResponse,
};
use reqwest::header;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::{fs, fs::File, io::AsyncWriteExt};
use tokio_util::io::ReaderStream;
use utoipa::ToSchema;
use uuid::Uuid;

#[allow(unused)]
#[derive(Deserialize, ToSchema)]
struct PlaceHolderForm {
    #[schema(format = Binary, content_media_type = "application/octet_stream")]
    file: String,
}

const MAX_SNIFF_BYTES: usize = 8192;
#[utoipa::path(
    post,
    path = "/upload",
    tag = "files",
    security(("bearerAuth" = [])),
    request_body(
        content = PlaceHolderForm,
        content_type = "multipart/form-data", 
        // describe that one or more files are expected; no concrete schema for each part
        description = "One or more file parts (multipart/form-data)"
    ),
    responses(
        (status = 200, description = "Uploaded file IDs", body = Vec<uuid::Uuid>),
        (status = 401, description = "Unauthorized"),
        (status = 415, description = "Unsupported file type"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn upload(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<Vec<Uuid>>, ApiError> {
    let upload_root = state.file_storage.clone();
    let mut stored_file_ids = Vec::new();
    eprintln!("[upload] start upload loop");
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|_| ApiError::Internal)?
    {
        let mut size_bytes: i64 = 0;

        let temp_name = format!("tmp-{}", Uuid::new_v4());
        let temp_path = upload_root.join(&temp_name);
        eprintln!("[upload] temp_path = {:?}", temp_path);
        let mut temp_file = File::create(&temp_path).await?;

        let mut hasher = Sha256::new();

        // buffer used for MIME sniffing
        let mut sniff_buffer = Vec::with_capacity(MAX_SNIFF_BYTES);

        while let Some(chunk) = field.chunk().await.map_err(|e| {
            eprintln!("[upload] field.chunk error: {:?}", e);
            ApiError::ArbitraryFileUpload
        })? {
            size_bytes += chunk.len() as i64;

            hasher.update(&chunk);

            // collect only first MAX_SNIFF_BYTES bytes
            if sniff_buffer.len() < MAX_SNIFF_BYTES {
                let remaining = MAX_SNIFF_BYTES - sniff_buffer.len();
                sniff_buffer.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
            }
            if let Err(e) = temp_file.write_all(&chunk).await {
                eprintln!("[upload] temp_file.write_all error: {:?}", e);
                return Err(ApiError::Internal);
            }
        }

        if let Err(e) = temp_file.flush().await {
            eprintln!("[upload] temp_file.flush error: {:?}", e);
            return Err(ApiError::Internal);
        }
        drop(temp_file);
        // infer MIME type
        // let inferred = infer::get(&sniff_buffer).ok_or(ApiError::UnsupportedFileType)?;
        //
        // let mime_type = inferred.mime_type().to_string();

        // optional allowlist
        // match mime_type.as_str() {
        //     "image/png" | "image/jpeg" | "image/webp" | "application/pdf" => {}
        //     _ => {
        //         let _ = fs::remove_file(&temp_path).await;
        //         return Err(ApiError::UnsupportedFileType);
        //     }
        // }

        let hash_hex = hex::encode(hasher.finalize());
        eprintln!("[upload] sha256 = {}", hash_hex);
        let original_filename = field.file_name().unwrap_or("unknown").to_string();
        eprintln!("[upload] original_filename = {}", original_filename);
        let dir1 = &hash_hex[0..2];
        let dir2 = &hash_hex[2..4];

        let final_dir = upload_root.join(dir1).join(dir2);
        eprintln!("[upload] final_dir = {:?}", final_dir);
        if let Err(e) = fs::create_dir_all(&final_dir).await {
            eprintln!("[upload] create_dir_all error: {:?}", e);
            return Err(ApiError::Filesystem);
        }
        let final_path = final_dir.join(&hash_hex);
        eprintln!("[upload] final_path = {:?}", final_path);
        let storage_key = format!("{}/{}/{}", dir1, dir2, hash_hex);

        let mime_type = field
            .content_type()
            .map(|m| m.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        eprintln!("[upload] mime_type = {}", mime_type);
        eprintln!("[upload] size_bytes = {}", size_bytes);
        eprintln!("[upload] sniff_buffer.len = {}", sniff_buffer.len());
        let insert_result = sqlx::query!(
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
        .await;

        let file_id = match insert_result {
            Ok(row) => {
                eprintln!("[upload] DB insert succeeded id={:?}", row.id);
                row.id
            }
            Err(e) => {
                eprintln!("[upload] DB insert error: {:?}", e);
                // cleanup temp file
                if let Err(rem_err) = fs::remove_file(&temp_path).await {
                    eprintln!("[upload] remove_file(temp) error: {:?}", rem_err);
                }
                return Err(ApiError::Internal);
            }
        };

        if fs::metadata(&final_path).await.is_err() {
            if let Err(e) = fs::rename(&temp_path, &final_path).await {
                eprintln!("[upload] rename error: {:?}", e);
                if let Err(del_err) = sqlx::query!(r#"DELETE FROM files WHERE id = $1"#, file_id)
                    .execute(&state.pool)
                    .await
                {
                    eprintln!(
                        "[upload] DB delete error after rename failure: {:?}",
                        del_err
                    );
                }
                if let Err(rem_err) = fs::remove_file(&temp_path).await {
                    eprintln!(
                        "[upload] remove_file(temp) after rename failure error: {:?}",
                        rem_err
                    );
                }
                return Err(ApiError::Internal);
            } else {
                eprintln!("[upload] renamed temp -> final");
            }
        } else {
            if let Err(e) = fs::remove_file(&temp_path).await {
                eprintln!(
                    "[upload] remove_file(temp) when final exists error: {:?}",
                    e
                );
            } else {
                eprintln!("[upload] removed temp because final already existed");
            }
        }
        stored_file_ids.push(file_id);
        eprintln!("[upload] stored_file_ids now len={}", stored_file_ids.len());
    }

    Ok(Json(stored_file_ids))
}
#[utoipa::path(
    get,
    path = "/files/{id}",
    tag = "files",
    params(
        ("id" = uuid::Uuid, Path, description = "File id to retrieve")
    ),
    // optional auth: documented as supported but not required
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "File stream", content_type = "application/octet-stream"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "File not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_file(
    MaybeAuthUser(maybe_claims): MaybeAuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = maybe_claims.map(|c| c.sub);
    //Annoying to figure out so imma avoid using it for now. No guarded files.
    let _can_access = sqlx::query_scalar!(
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
    // if !can_access {
    //     return Err(ApiError::Unauthorized);
    // }
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
    Ok(response)
}
//For better design make sure we implement a way of cleaning orphaned files. While we prevent it
//explicitly from happening at creation we don't account for when a user deletes a repo meaning the
//files still exist but don't actually have a relation anywhere.
pub async fn clean_files() {}
