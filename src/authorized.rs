use actix_web::{
    HttpMessage, HttpResponse, Result,
    body::{EitherBody, MessageBody},
    dev::{ServiceRequest, ServiceResponse},
    http::header,
    middleware::Next,
};
use futures::TryFutureExt;
use hmac::{Hmac, digest::KeyInit};
use jwt::VerifyWithKey;
use serde::Deserialize;
use sha2::Sha256;

#[derive(Debug, Deserialize)]
pub struct AuthPayload {
    #[allow(unused)]
    permissions: Vec<String>,
}

impl AuthPayload {
    pub fn permissions(&self) -> &[String] {
        &self.permissions
    }
}

pub async fn is_authorized(
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<EitherBody<impl MessageBody>>> {
    let auth_token = match req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
    {
        Some(auth_header) if auth_header.starts_with("Bearer ") => {
            // 7 is the length of "Bearer "
            &auth_header[7..]
        }

        _ => {
            return Ok(
                // this is fun syntax, I had fun writing this actually
                req.into_response(HttpResponse::Unauthorized().finish().map_into_right_body()),
            );
        }
    };

    let hmac: Hmac<Sha256> = Hmac::new_from_slice(b"key").unwrap();
    let Ok(payload): Result<AuthPayload, _> = auth_token.verify_with_key(&hmac) else {
        return Ok(req.into_response(HttpResponse::Forbidden().finish().map_into_right_body()));
    };

    // insert the payload into the request extensions for later use, if wanted
    req.extensions_mut().insert(payload);

    next.call(req)
        .map_ok(ServiceResponse::map_into_left_body)
        .await
}
