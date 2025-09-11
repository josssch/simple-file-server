use actix_multipart::form::{MultipartForm, tempfile::TempFile};
use actix_web::{HttpRequest, Responder, post, web};

#[derive(Debug, MultipartForm)]
struct UploadFileForm {
    file: TempFile,
}

#[post("/upload/{path:.*}")]
pub async fn upload_file(
    req: HttpRequest,
    path: web::Path<String>,
    MultipartForm(form): MultipartForm<UploadFileForm>,
) -> impl Responder {
    let file_path = path.into_inner();
    let temp_file = form.file;

    "File upload endpoint"
}
