use std::{
    fs::File,
    io::{BufReader, Cursor, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use actix_web::{
    HttpRequest, HttpResponse, HttpResponseBuilder, Responder, get,
    http::header::{self, CacheControl, CacheDirective, ContentType},
    mime,
    web::{self, Bytes, Data, Query},
};
use async_stream::stream;
use futures::{Stream, TryStreamExt};
use path_clean::PathClean;
use serde::{Deserialize, Deserializer};

use crate::{
    config::server::{FileSource, ServerConfig},
    state::{CachedFileEntry, FileCache},
};

fn stream_web_bytes<R: Read + Send + 'static>(
    source: R,
) -> impl Stream<Item = actix_web::Result<Bytes>> {
    stream_raw_bytes(source).map_ok(|a| Bytes::copy_from_slice(&a))
}

fn stream_raw_bytes<R: Read + Send + 'static>(
    source: R,
) -> impl Stream<Item = actix_web::Result<Vec<u8>>> {
    let mut reader = BufReader::new(source);

    stream! {
        let mut buffer = [0; 8192];
        loop {
            let bytes_read = match reader.read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => n,
                Err(_) => {
                    yield Err(actix_web::error::ErrorInternalServerError("Failed to read file"));
                    break;
                }
            };

            yield Ok(Vec::from(&buffer[..bytes_read]));
        }
    }
}

fn string_bool<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let s = String::deserialize(d)?;
    match &*s.to_ascii_lowercase() {
        // empty string is also true, since its presence is enough
        "" | "y" | "yes" | "t" | "true" | "1" => Ok(true),
        _ => Ok(false),
    }
}

fn secure_virtual_path(base: &Path, user_input: &str) -> Option<PathBuf> {
    let combined = base.join(user_input).clean();

    // ensure the final cleaned path is still within base
    if combined.starts_with(base) {
        Some(combined)
    } else {
        None
    }
}

#[derive(Deserialize)]
struct FileOptions {
    #[serde(default, alias = "dl", deserialize_with = "string_bool")]
    download: bool,
}

#[get("/{file_name:.*}")]
pub async fn serve_file(
    req: HttpRequest,
    path: web::Path<String>,
    query: Query<FileOptions>,
) -> impl Responder {
    let file_name = path.into_inner();

    let config = req
        .app_data::<Data<ServerConfig>>()
        .expect("missing server config file");

    let file_cache = req.app_data::<FileCache>().expect("missing file cache");

    match &config.files_source {
        FileSource::Local { base_dir } => {
            let Some(file_path) = secure_virtual_path(base_dir.as_ref(), &file_name) else {
                return HttpResponse::Forbidden().body("Access to the specified path is forbidden");
            };

            if !file_path.exists() || !file_path.is_file() || file_path.is_symlink() {
                return HttpResponse::NotFound().body("File does not exist");
            }

            // create our initial response builder with common headers, then customize it later
            let mut builder = HttpResponse::Ok()
                .insert_header((header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"))
                .content_type(if query.download {
                    ContentType::octet_stream()
                } else {
                    // try to guess mime type from file extension, except HTML files to prevent
                    // rendering, default to text/plain; charset=utf-8
                    ContentType(
                        mime_guess::from_path(&file_path)
                            .first()
                            .filter(|m| m.subtype() != mime::HTML)
                            .unwrap_or(mime::TEXT_PLAIN_UTF_8),
                    )
                })
                .take();

            // if the file is cached, serve it from memory instead of disk
            if let Some(file) = file_cache.lock().await.get(&file_path).cloned() {
                return response_from_file(&req, &file, builder);
            }

            // open the file and open a stream to it
            let Ok(file_bytes_stream) = File::open(&file_path).map(stream_raw_bytes) else {
                return HttpResponse::InternalServerError().body("Failed to open file");
            };

            // checking if the file size is greater than the threshold, if it is, we won't cache it
            if config.memory_cache.enabled
                && let Ok(metadata) = file_path.metadata()
                && metadata.len() <= config.memory_cache.max_size_bytes
            {
                // in order to cache the file, we need to stream all its bytes into memory first
                // (this is only okay because we already checked the file size above)
                let Ok(all_bytes) = file_bytes_stream.try_concat().await else {
                    return HttpResponse::InternalServerError().body("Failed to read file");
                };

                let file_entry = CachedFileEntry::new(all_bytes);
                let response = response_from_file(&req, &file_entry, builder);

                // now finally insert the cached entry into the cache
                file_cache.lock().await.insert(file_path, file_entry);

                return response;
            }

            // if we get here, it means we are not caching the file, so just stream it directly
            // and instead of an ETag, we add Cache-Control headers since calculating the hash
            // would require reading the entire file anyway, which I want to avoid
            // todo: once I introduce an api for upload, I will store file information at that time
            builder
                .insert_header(CacheControl(vec![
                    CacheDirective::Public,
                    CacheDirective::MaxAge(60 * 60), // 1 hour
                ]))
                .streaming(file_bytes_stream.map_ok(|b| Bytes::copy_from_slice(&b)))
        }
    }
}

fn response_from_file(
    req: &HttpRequest,
    file: &CachedFileEntry,
    mut base_response: HttpResponseBuilder,
) -> HttpResponse {
    let etag = req
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok());

    if etag.is_some_and(|t| t == file.hash()) {
        return HttpResponse::NotModified().finish();
    }

    let cursor = Cursor::new(Arc::clone(file.bytes()));

    return base_response
        .insert_header((header::ETAG, file.hash().to_string()))
        .streaming(stream_web_bytes(cursor));
}
