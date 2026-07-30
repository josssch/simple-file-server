use std::{io::Read, iter, path::Path};

use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope,
    dev::HttpServiceFactory,
    error, guard,
    http::header::{self, ContentType},
    middleware::Compress,
    mime,
    web::{self, Bytes, Data, Query},
};
use futures::stream;
use serde::{Deserialize, Deserializer};

use crate::{SharedFileStore, routes::ScopeCreator};

pub struct FileServeRoute;

impl ScopeCreator for FileServeRoute {
    fn create_scope() -> impl HttpServiceFactory {
        Scope::new("").wrap(Compress::default()).route(
            "/{file_path:.*}",
            web::get()
                .guard(guard::fn_guard(not_api_path))
                .to(serve_file),
        )
    }
}

fn not_api_path(ctx: &guard::GuardContext<'_>) -> bool {
    let path = ctx.head().uri.path();
    let trimmed = path.trim_start_matches('/');

    if trimmed.is_empty() {
        return true;
    }

    let first_segment = trimmed.split('/').next().unwrap_or("");
    first_segment != "api"
}

fn string_bool<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let s = String::deserialize(d)?;
    match &*s.to_ascii_lowercase() {
        // empty string is also true, since its presence is enough
        "" | "y" | "yes" | "t" | "true" | "1" => Ok(true),
        _ => Ok(false),
    }
}

#[derive(Deserialize)]
struct FileOptions {
    #[serde(default, alias = "dl", deserialize_with = "string_bool")]
    download: bool,
}

async fn serve_file(
    req: HttpRequest,
    path: web::Path<String>,
    query: Query<FileOptions>,
    store: Data<SharedFileStore>,
) -> impl Responder {
    let file_path = path.into_inner();

    let Some(file) = store.get_file(Path::new(&file_path)) else {
        return HttpResponse::NotFound().body("File does not exist");
    };

    let file = file.as_ref();
    let hash = &file.metadata().hash;

    if let Some(etag) = req
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        && etag == hash
    {
        return HttpResponse::NotModified().finish();
    }

    let mut reader = match file.open_reader() {
        Ok(reader) => reader,
        Err(err) => {
            eprintln!("Error opening file for read: {err}");
            return HttpResponse::InternalServerError().body("Failed to read file");
        }
    };

    let mut buffer = [0u8; 8192];
    let mut is_failed = false;

    let bytes_iter = iter::from_fn(move || {
        if is_failed {
            return None;
        }

        let bytes_read = match Read::read(&mut *reader, &mut buffer) {
            Ok(0) => return None,
            Ok(n) => n,
            Err(err) => {
                is_failed = true;
                return Some(Err(err));
            }
        };

        Some(Ok(Vec::from(&buffer[..bytes_read])))
    });

    HttpResponse::Ok()
        .insert_header((header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"))
        .insert_header((header::ETAG, hash.to_string()))
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
        .streaming(stream::iter(bytes_iter.map(|r| {
            r.as_ref()
                .map(|b| Bytes::copy_from_slice(b))
                .map_err(|_| error::ErrorInternalServerError("File read error"))
        })))
}
