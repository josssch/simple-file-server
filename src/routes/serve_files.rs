use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope,
    dev::HttpServiceFactory,
    error, get,
    http::header::{self, ContentType},
    middleware::Compress,
    mime,
    web::{self, Bytes, Data, Query},
};
use futures::stream;
use serde::{Deserialize, Deserializer};

use crate::{
    config::server::{FileSource, ServerConfig},
    file_store::{FileStore, FsFileStore, ServeableFile},
    routes::ScopeCreator,
};

pub struct FileServeRoute;

impl ScopeCreator for FileServeRoute {
    fn create_scope() -> impl HttpServiceFactory {
        Scope::new("").wrap(Compress::default()).service(serve_file)
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

#[derive(Deserialize)]
struct FileOptions {
    #[serde(default, alias = "dl", deserialize_with = "string_bool")]
    download: bool,
}

#[get("/{file_path:.*}")]
pub async fn serve_file(
    req: HttpRequest,
    path: web::Path<String>,
    query: Query<FileOptions>,
    config: Data<ServerConfig>,
) -> impl Responder {
    let file_path = path.into_inner();

    match &config.files_source {
        FileSource::Local { base_dir } => {
            let store = FsFileStore::new(base_dir);
            let Some(file) = store.get_file(&file_path) else {
                return HttpResponse::NotFound().body("File does not exist");
            };

            let file = file.as_ref();
            let hash = file.metadata().hash();

            if let Some(etag) = req
                .headers()
                .get(header::IF_NONE_MATCH)
                .and_then(|v| v.to_str().ok())
                && etag == hash
            {
                return HttpResponse::NotModified().finish();
            }

            let bytes_iter = file.bytes_iter();

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
    }
}
