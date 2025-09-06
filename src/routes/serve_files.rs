use std::{
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

use actix_web::{
    HttpRequest, HttpResponse, Responder, get,
    http::header::{CacheControl, CacheDirective, ContentType},
    mime,
    web::{self, Data, Query},
};
use async_stream::stream;
use futures::Stream;
use path_clean::PathClean;
use serde::{Deserialize, Deserializer};

use crate::config::server::{FileSource, ServerConfig};

fn stream_contents<R: Read>(source: R) -> impl Stream<Item = actix_web::Result<web::Bytes>> {
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

            yield Ok(web::Bytes::copy_from_slice(&buffer[..bytes_read]));
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

    match &config.files_source {
        FileSource::Local { base_dir } => {
            let Some(file_path) = secure_virtual_path(base_dir.as_ref(), &file_name) else {
                return HttpResponse::Forbidden().body("Access to the specified path is forbidden");
            };

            if !file_path.exists() || !file_path.is_file() || file_path.is_symlink() {
                return HttpResponse::NotFound().body("File does not exist");
            }

            let Ok(file) = File::open(&file_path) else {
                return HttpResponse::InternalServerError().body("Failed to open file");
            };

            HttpResponse::Ok()
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
                .streaming(stream_contents(file))
        }
    }
}
