mod authorized;
mod cache_map;
mod config;
mod file_store;
mod routes;

use std::io;

use actix_web::{App, HttpServer, web::Data};

use crate::{
    config::server::ServerConfig,
    routes::{ScopeCreator, api::ApiRoute, serve_files::FileServeRoute},
};

#[actix_web::main]
async fn main() -> io::Result<()> {
    let mut config_file = ServerConfig::new_file();
    config_file.read_and_save()?;

    let config = config_file.take().expect("just read from file");
    let binding = (config.host.clone(), config.port);

    println!("Starting server at http://{}:{}", config.host, config.port);

    let config_data: Data<ServerConfig> = Data::new(config);

    HttpServer::new(move || {
        // moving config_data into here, to be cloned each time a new worker is spawned
        // (which is what this function closure is for generating)
        App::new()
            .app_data(config_data.clone())
            .service(ApiRoute::create_scope())
            .service(FileServeRoute::create_scope())
    })
    .bind(binding)?
    .run()
    .await
}
