use actix_web::HttpResponse;

pub const HEALTH_CHECK_PATH: &'static str = "/health-check";

pub async fn health_check() -> HttpResponse {
    HttpResponse::Ok().finish()
}
