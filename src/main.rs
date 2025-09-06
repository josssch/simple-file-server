mod cache_map;
mod config;
mod routes;
mod state;

use std::io;

use actix_web::{App, HttpServer, middleware::Compress, web::Data};
use futures::lock::Mutex;

use crate::{
    cache_map::CacheMap, config::server::ServerConfig, routes::serve_files::serve_file,
    state::FileCache,
};

#[actix_web::main]
async fn main() -> io::Result<()> {
    let mut config_file = ServerConfig::new_file();
    config_file.read_from_file()?;

    let config = config_file.take().expect("just read from file");
    let binding = (config.host.clone(), config.port);

    let file_cache = FileCache::new(Mutex::new(CacheMap::new()));

    println!("Starting server at http://{}:{}", config.host, config.port);

    let config_data = Data::new(config);
    HttpServer::new(move || {
        // moving config_data into here, to be cloned each time a new worker is spawned
        // (which is what this function closure is for generating)
        App::new()
            .app_data(config_data.clone())
            .app_data(file_cache.clone())
            .wrap(Compress::default())
            .service(serve_file)
    })
    .bind(binding)?
    .run()
    .await
}
