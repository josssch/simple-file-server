use actix_multipart::form::MultipartFormConfig;
use actix_web::{Scope, dev::HttpServiceFactory, middleware};

use crate::{
    authorized::is_authorized,
    routes::{
        ScopeCreator,
        upload_file::{delete_file, upload_file},
    },
};

pub struct ApiRoute;

impl ScopeCreator for ApiRoute {
    fn create_scope() -> impl HttpServiceFactory {
        Scope::new("/api")
            .app_data(MultipartFormConfig::default())
            .wrap(middleware::from_fn(is_authorized))
            .service(upload_file)
            .service(delete_file)
    }
}
