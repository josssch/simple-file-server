use std::{io::BufReader, path::PathBuf};

use actix_multipart::form::{MultipartForm, tempfile::TempFile};
use actix_web::{
    HttpResponse, Responder, post,
    web::{self, Data},
};

use crate::SharedFileStore;

#[derive(Debug, MultipartForm)]
struct UploadFileForm {
    file: TempFile,
}

#[post("/upload/{path:.*}")]
pub async fn upload_file(
    path: web::Path<String>,
    MultipartForm(form): MultipartForm<UploadFileForm>,
    file_store: Data<SharedFileStore>,
) -> impl Responder {
    let path = PathBuf::from(path.into_inner());

    match file_store.upload(&path, BufReader::new(form.file.file.into_file())) {
        Ok(_) => HttpResponse::Created().finish(),
        Err(err) => {
            eprintln!("Error uploading file: {err}");
            HttpResponse::InternalServerError().body("Failed to upload file")
        }
    }
}
