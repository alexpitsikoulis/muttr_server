use actix_web::{
    web::{Data, Form},
    HttpResponse,
};
use secrecy::Secret;
use serde::Deserialize;
use sqlx::PgPool;

use crate::{
    domain::user::Login,
    storage::{get_user_by_email, get_user_by_handle, upsert_user},
    utils::jwt::generate_token,
};

pub const LOGIN_PATH: &str = "/login";

#[derive(Deserialize)]
pub struct LoginForm {
    pub login: Login,
    pub password: Secret<String>,
}

#[tracing::instrument(
    name = "Logging in user",
    skip(form, db_pool),
    fields(
        user_login_option = %form.login,
    )
)]
pub async fn login(form: Form<LoginForm>, db_pool: Data<PgPool>) -> HttpResponse {
    let query_result = match &form.login {
        Login::Email(e) => get_user_by_email(db_pool.get_ref(), e.as_ref()).await,
        Login::Handle(h) => get_user_by_handle(db_pool.get_ref(), h.as_ref()).await,
    };
    match query_result {
        Ok(mut user) => {
            if !user.email_confirmed() {
                return HttpResponse::Unauthorized().body("Account email has not been confirmed");
            }
            if user.failed_attempts() >= 10 {
                return HttpResponse::Forbidden()
                    .body("Account is locked due to too many failed login attempts");
            };
            let login_successful = user.password().compare(&form.password);
            if login_successful {
                if user.failed_attempts() > 0 {
                    if let Err(e) = upsert_user(db_pool.get_ref(), &user).await {
                        tracing::error!("INSERT into users table failed: {:?}", e);
                        return HttpResponse::InternalServerError().finish();
                    }
                    user.reset_failed_attempts();
                }
            } else {
                if let Err(e) = upsert_user(db_pool.get_ref(), &user).await {
                    tracing::error!("INSERT into users table failed: {:?}", e);
                    return HttpResponse::InternalServerError().finish();
                }
                user.increment_failed_attempts();
                return HttpResponse::Ok().finish();
            }

            match generate_token(user.id()) {
                Ok(token) => HttpResponse::Ok()
                    .append_header(("Authorization", token))
                    .finish(),
                Err(e) => {
                    tracing::error!("Failed to generate JWT: {:?}", e);
                    HttpResponse::InternalServerError().finish()
                }
            }
        }
        Err(e) => match e {
            sqlx::Error::RowNotFound => HttpResponse::Ok().finish(),
            _ => {
                tracing::error!("Failed to execute GET from users table query: {:?}", e);
                HttpResponse::InternalServerError().finish()
            }
        },
    }
}
