use std::{
    fs::File,
    io::{BufReader, Cursor, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use actix_web::{
    HttpRequest, HttpResponse, Responder, get,
    http::header::{CacheControl, CacheDirective, ContentType},
    mime,
    web::{self, Bytes, Data, Query},
};
use async_stream::stream;
use futures::{Stream, TryStreamExt};
use path_clean::PathClean;
use serde::{Deserialize, Deserializer};

use crate::{
    config::server::{FileSource, ServerConfig},
    state::{FileCache, SharedBytes},
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
                .insert_header(CacheControl(vec![
                    CacheDirective::Public,
                    CacheDirective::MaxAge(60 * 60), // 1 hour
                ]))
                .content_type(if query.download {
                    ContentType::octet_stream()
                } else {
                    // try to guess mime type from file extension, default to text/plain; charset=utf-8
                    ContentType(mime_guess::from_path(&file_path).first_or(mime::TEXT_PLAIN_UTF_8))
                })
                .take();

            let bytes = file_cache.lock().await.get(&file_path).cloned();
            match bytes {
                // if the file is cached, serve it from memory
                Some(bytes) => {
                    let cursor = Cursor::new(Arc::clone(&*bytes));
                    builder.streaming(stream_web_bytes(cursor))
                }

                // if not cached, read from disk and cache it if its size is <= 10MB
                None => {
                    let Ok(file) = File::open(&file_path) else {
                        return HttpResponse::InternalServerError().body("Failed to open file");
                    };

                    let file_stream = stream_raw_bytes(file);

                    // checking if the file size is greater than 10MB, if it is, we won't cache it
                    if let Ok(metadata) = file_path.metadata()
                        && metadata.len() <= 1000 * 1000 * 10
                    {
                        // read all bytes into memory to cache it
                        let Ok(all_bytes) = file_stream.try_concat().await else {
                            return HttpResponse::InternalServerError().body("Failed to read file");
                        };

                        let shared_bytes = SharedBytes::new(all_bytes);
                        file_cache
                            .lock()
                            .await
                            .insert(file_path, shared_bytes.clone());

                        let cursor = Cursor::new(Arc::clone(&*shared_bytes));
                        builder.streaming(stream_web_bytes(cursor))
                    } else {
                        builder.streaming(file_stream.map_ok(|a| Bytes::copy_from_slice(&a)))
                    }
                }
            }
        }
    }
}
