use std::{
    io::{self, BufReader},
    path::PathBuf,
};

use actix_multipart::form::{MultipartForm, tempfile::TempFile};
use actix_web::{
    HttpResponse, Responder, delete, get, post,
    web::{self, Data},
};

use crate::SharedFileStore;

#[derive(Debug, MultipartForm)]
struct UploadFileForm {
    file: TempFile,
}

// I would love for these routes to only have different HTTP methods
// with the same path (i.e. GET /:file, POST /:file, and DELETE /:file).
// However, due to there needing to be different guards/middleware per these routes
// and an issue with actix-web (https://github.com/actix/actix-web/issues/2904), it can't happen
// without some hackery on my part, which I don't want to do right now

#[post("/{path:.*}")]
pub async fn upload_file(
    path: web::Path<String>,
    MultipartForm(form): MultipartForm<UploadFileForm>,
    file_store: Data<SharedFileStore>,
) -> impl Responder {
    let path = PathBuf::from(path.into_inner());
    let mut reader = BufReader::new(form.file.file);

    match file_store.upload(&path, &mut reader) {
        Ok(_) => HttpResponse::Created().finish(),
        Err(err) if err.kind() == io::ErrorKind::InvalidInput => {
            HttpResponse::BadRequest().body(format!("Invalid input: {err}"))
        }
        Err(err) => {
            eprintln!("Error uploading file: {err}");
            HttpResponse::InternalServerError().body("Failed to upload file")
        }
    }
}

#[delete("/{path:.*}")]
pub async fn delete_file(
    path: web::Path<String>,
    file_store: Data<SharedFileStore>,
) -> impl Responder {
    let path = PathBuf::from(path.into_inner());

    match file_store.remove(&path) {
        Ok(_) => HttpResponse::Ok().body("File deleted"),
        Err(err) if err.kind() == io::ErrorKind::InvalidInput => {
            HttpResponse::BadRequest().body(format!("Invalid input: {err}"))
        }
        Err(err) => {
            eprintln!("Error deleting file: {err}");
            HttpResponse::InternalServerError().body("Failed to delete file")
        }
    }
}

#[get("/files")]
pub async fn list_files(file_store: Data<SharedFileStore>) -> impl Responder {
    match file_store.list() {
        Ok(files) => HttpResponse::Ok().json(files),
        Err(err) => {
            eprintln!("Error listing files: {err}");
            HttpResponse::InternalServerError().body("Failed to list files")
        }
    }
}
