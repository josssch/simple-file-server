use std::{
    any::{Any, TypeId},
    fs::File,
    io::{BufReader, Read, Write},
    path::PathBuf,
};

use actix_multipart::form::{MultipartForm, tempfile::TempFile};
use actix_web::{
    HttpRequest, HttpResponse, Responder, post,
    web::{self, Data},
};
use sha2::{Digest, Sha256};

use crate::{
    SharedFileStore,
    file_store::{FileMetadata, FsFileStore, METADATA_FILE_EXT},
};

#[derive(Debug, MultipartForm)]
struct UploadFileForm {
    file: TempFile,
}

#[post("/upload/{path:.*}")]
pub async fn upload_file(
    req: HttpRequest,
    path: web::Path<String>,
    MultipartForm(form): MultipartForm<UploadFileForm>,
    file_store: Data<SharedFileStore>,
) -> impl Responder {
    // if (**file_store.get_ref()).type_id() != TypeId::of::<FsFileStore>() {
    //     return HttpResponse::NotImplemented().finish();
    // }

    let file_path = PathBuf::from("files").join(PathBuf::from(path.into_inner()));
    let temp_file = form.file;

    let mut target_file = File::create(&file_path).unwrap();

    let mut reader = BufReader::new(&temp_file.file);
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => todo!(),
        };

        let bytes = &buffer[..n];

        digest.update(bytes);
        target_file.write(bytes);
    }

    let hex_hash = FileMetadata::hash_to_hex(digest);
    let metadata = FileMetadata {
        hash: hex_hash,
        size_bytes: temp_file.size as u64,
    };

    let metadata_file = File::create(file_path.with_extension("metadata.json"));
    serde_json::to_writer(metadata_file.unwrap(), &metadata).unwrap();

    HttpResponse::Created().finish()
}
