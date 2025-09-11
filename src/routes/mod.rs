use actix_web::dev::HttpServiceFactory;

pub mod api;
pub mod serve_files;
pub mod upload_file;

pub trait ScopeCreator {
    fn create_scope() -> impl HttpServiceFactory;
}
