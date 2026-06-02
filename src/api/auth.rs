use std::time::SystemTime;

use axum::{
    Extension, Json, Router,
    body::Body,
    extract::Query,
    response::{IntoResponse, Response},
    routing::{get, post, post_service},
};
use base64::{display::Base64Display, prelude::BASE64_STANDARD};
use bytes::Bytes;
use compact_str::CompactString;
use http::{StatusCode, Uri, header, response::Parts};
use lettre::{
    Message,
    message::{Mailbox, SinglePart},
};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use tower_sessions_core::session::Id;

use crate::{
    bad,
    libs::{
        auth::{
            EmailVerificationCodeType, Encoded, Session_, availability, decode2, email_check,
            get_code, get_email_content,
        },
        constants::{
            APPLICATION_JAVASCRIPT_UTF_8, APPLICATION_JSON_UTF_8, BYTES_EMPTY, BYTES_NULL,
            DELAY_FOR_SECURITY, PASSWORD_LENGTH,
        },
        db::{DBError, JsonChecked, get_connection, insert_connection},
        email::{get_source, send_mail},
        geoip::{Ip, in_china},
        preference::server::PreferenceConfig,
        privilege,
        request::{JsonReqult, RawPayload, Repult},
        response::JkmxJsonResponse,
        serde::WithJson,
        session,
        validate::{check_email, check_uid, check_username},
    },
    models::{
        group::GroupA,
        user::{User, UserA},
    },
};

const NO_SUCH_USER: JkmxJsonResponse = JkmxJsonResponse::Response(
    StatusCode::OK,
    Bytes::from_static(br#"{"error":"NO_SUCH_USER"}"#),
);
const WRONG_PASSWORD: JkmxJsonResponse = JkmxJsonResponse::Response(
    StatusCode::OK,
    Bytes::from_static(br#"{"error":"WRONG_PASSWORD"}"#),
);

mod private {
    use bytes::Bytes;
    use serde_json::{Serializer as JSerializer, ser::CompactFormatter};
    use std::io::Write;

    pub(super) trait Δ: serde::Serializer {
        fn δ(_: &Bytes, _: Self) -> Result<Self::Ok, Self::Error>;
    }

    impl<S: serde::Serializer> Δ for S {
        default fn δ(_: &Bytes, _: Self) -> Result<Self::Ok, Self::Error> {
            // Won't be instantiated.
            unimplemented!("Not implemented intentionally.");
        }
    }

    impl Δ for &mut JSerializer<&mut Vec<u8>, CompactFormatter> {
        fn δ(data: &Bytes, serializer: Self) -> Result<Self::Ok, Self::Error> {
            serializer.as_inner().0.write_all(data).map_err(serde_json::Error::io)
        }
    }

    pub(super) fn err() -> super::JkmxJsonResponse {
        let err = super::DBError::new(tokio_postgres::error::Kind::RowCount, Some("database insertion error".into()));
        return super::JkmxJsonResponse::Error(super::StatusCode::INTERNAL_SERVER_ERROR, err.into());
    }
}

#[derive(Deserialize)]
struct SessionInfoRequest {
    jsonp: Option<CompactString>,
    token: Option<String>,
}

#[derive(Serialize)]
struct ServerVersion {
    hash: &'static str,
    date: u64,
}

impl const Default for ServerVersion {
    fn default() -> Self {
        Self {
            hash: env!("SERVER_VERSION_HASH"),
            date: const {
                if let Ok(date) = u64::from_str_radix(env!("SERVER_VERSION_DATE"), 10) {
                    date * 1000
                } else {
                    panic!("Invalid SERVER_VERSION_DATE");
                }
            },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionInfoResponse {
    server_preference: PreferenceConfig,
    server_version: ServerVersion,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_meta: Option<UserA>,
    #[serde(skip_serializing_if = "Option::is_none")]
    joined_groups_count: Option<u64>,
    user_privileges: privilege::Privileges,
    #[serde(serialize_with = "private::Δ::δ")]
    user_preference: Bytes,
    #[serde(skip_serializing_if = "Option::is_none")]
    extra_error: Option<&'static str>,
}

unsafe impl Send for SessionInfoResponse {}

async fn get_session_info(
    ip: Ip,
    Extension(now): Extension<SystemTime>,
    req: Repult<Query<SessionInfoRequest>>,
) -> Response {
    const JSONP_HEAD: &str = "(globalThis.getSessionInfoCallback??(e=>globalThis.sessionInfo=e))(";
    const JSONP_TRAIL: &str = ");";
    const SQL_GET_PREF: &str = "select preference from lean4oj.user_preference where uid = $1";

    fn not_falsy(inner: CompactString) -> bool {
        !["false", "f", "no", "n", "off", "0"]
            .into_iter()
            .any(|s| inner.eq_ignore_ascii_case(s))
    }

    let Query(SessionInfoRequest { jsonp, token }) = match req {
        Ok(s) => s,
        Err(err) => return err.into_response(),
    };
    let jsonp = jsonp.is_some_and(not_falsy);

    let mut res = SessionInfoResponse {
        server_version: const { ServerVersion::default() },
        server_preference: const { PreferenceConfig::default() },
        user_meta: None,
        joined_groups_count: None,
        user_privileges: SmallVec::new(),
        user_preference: BYTES_EMPTY,
        extra_error: None,
    };

    if let Some(token) = token {
        let mut conn0 = None;
        let session = decode2(token.as_bytes(), now, &mut conn0).await;
        match session {
            Session_::Session(_) =>
                if let Ok(conn) = insert_connection(&mut conn0).await
                && let Ok(Some(user)) = User::from_session(session, conn).await {
                    res.joined_groups_count = GroupA::count(&user.uid, conn).await.ok();
                    res.user_privileges = privilege::all(&user.uid, conn).await.unwrap_or_default();
                    if let Ok(stmt) = conn.prepare_static(SQL_GET_PREF.into()).await
                    && let Ok(row) = conn.query_one(&stmt, &[&&*user.uid]).await
                    && let Ok(pref) = row.try_get::<_, JsonChecked>(0) {
                        res.user_preference = row.buffer_bytes().slice_ref(pref.0);
                    }
                    res.user_meta = Some(UserA { user, is_admin: privilege::is_admin(&res.user_privileges) });
                }
            Session_::Token(_) => res.extra_error = Some("TOKEN_DISALLOWED"),
            Session_::None => (),
        }
    }

    if let Some(ip) = ip.0 && in_china(ip) {
        res.server_preference.make_gravatar_cdn();
    }

    let mut body = if jsonp { JSONP_HEAD.to_owned() } else { String::new() };
    let _ = serde_json::to_writer(unsafe { body.as_mut_vec() }, &res);
    if jsonp { body.push_str(JSONP_TRAIL); }

    let mut res = Response::new(Body::from(body));
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        if jsonp { APPLICATION_JAVASCRIPT_UTF_8 } else { APPLICATION_JSON_UTF_8 },
    );
    res
}

#[derive(Deserialize)]
struct LoginRequest {
    identifier: Option<CompactString>,
    email: Option<CompactString>,
    password: String,
}

async fn login(req: JsonReqult<LoginRequest>) -> JkmxJsonResponse {
    const SQL_ID: &str = "select uid, username from lean4oj.users where uid = $1 and username != '' and password = $2";
    const SQL_EMAIL: &str = "select uid, username from lean4oj.users where username != '' and email = $1 and password = $2";

    let Json(LoginRequest { identifier, email, password }) = req?;
    if identifier.is_none() && email.is_none() { bad!(BYTES_NULL) }

    let mut conn = get_connection().await?;
    let row = if let Some(id) = identifier {
        let stmt = conn.prepare_static(SQL_ID.into()).await?;
        conn.query_one(&stmt, &[&&*id, &password]).await
    } else {
        let email = unsafe { email.unwrap_unchecked() };
        let stmt = conn.prepare_static(SQL_EMAIL.into()).await?;
        conn.query_one(&stmt, &[&&*email, &password]).await
    };
    let Ok(row) = row else { return WRONG_PASSWORD };

    let uid = row.try_get(0)?;
    let username = row.try_get::<_, &str>(1)?;
    let session = session::create(uid).await?;
    let encoded = Encoded::try_from(session.id().unwrap_or(Id(0)))?;
    let res = format!(r#"{{"token":"{}","username":"{username}"}}"#, Base64Display::new(encoded.as_ref(), &BASE64_STANDARD));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

async fn logout(session: Session_) -> JkmxJsonResponse {
    if let Session_::Session(session) = session {
        session.delete().await?;
    }
    JkmxJsonResponse::Response(StatusCode::OK, BYTES_NULL)
}

async fn check_availability(req: Uri) -> JkmxJsonResponse {
    let Some(query) = req.query() else { return JkmxJsonResponse::Response(StatusCode::OK, BYTES_NULL) };

    let res = match form_urlencoded::parse(query.as_bytes()).next() {
        Some((deref!("username"), _)) => const { Bytes::from_static(br#"{"usernameAvailable":true}"#) },
        Some((deref!("identifier"), id)) => {
            let mut conn = get_connection().await?;
            let a = availability::identifier(&id, &mut conn).await?;
            format!(r#"{{"identifierAvailable":{a}}}"#).into()
        }
        Some((deref!("email"), email)) => {
            let mut conn = get_connection().await?;
            let a = availability::email(&email, &mut conn).await?;
            format!(r#"{{"emailAvailable":{a}}}"#).into()
        }
        _ => BYTES_NULL,
    };

    JkmxJsonResponse::Response(StatusCode::OK, res)
}

#[derive(Deserialize)]
struct SendEmailRequest {
    email: CompactString,
    r#type: EmailVerificationCodeType,
    locale: Option<CompactString>,
}

async fn send_email(
    Extension(now): Extension<SystemTime>,
    req: JsonReqult<SendEmailRequest>,
) -> JkmxJsonResponse {
    const SQL_EMAIL: &str = "select username from lean4oj.users where username != '' and email = $1";

    let Json(SendEmailRequest { email, r#type, locale }) = req?;

    let Some((_, _, address)) = check_email(&email) else { bad!(BYTES_NULL) };

    let mut conn = get_connection().await?;
    let stmt = conn.prepare_static(SQL_EMAIL.into()).await?;
    let Ok(row) = conn.query_one(&stmt, &[&&*email]).await else { return NO_SUCH_USER };
    let username = row.try_get::<_, &str>(0)?;

    let record = match get_code(email, now) {
        Ok(r) => r,
        Err(e) => {
            let res = format!(r#"{{"error":"RATE_LIMITED","cd":{}}}"#, WithJson(e));
            return JkmxJsonResponse::Response(StatusCode::OK, res.into());
        }
    };

    let (subject, body) = get_email_content(r#type, locale.as_deref(), record.code);

    let message = Message::builder()
        .from(get_source())
        .to(Mailbox::new(Some(username.to_owned()), address))
        .subject(subject.to_owned())
        .singlepart(SinglePart::html(body))?;

    send_mail(message).await?;

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

#[derive(Deserialize)]
struct RegisterRequest {
    username: CompactString,
    identifier: CompactString,
    email: CompactString,
    password: String,
}

async fn register(
    Extension(now): Extension<SystemTime>,
    req: JsonReqult<RegisterRequest>,
) -> JkmxJsonResponse {
    const SQL_USERS: &str = "insert into lean4oj.users (uid, username, email, password, register_time, avatar_info) values ($1, $2, $3::text, $4, $5, 'gravatar:' || $3::text)";
    const SQL_USER_INFORMATION: &str = "insert into lean4oj.user_information (uid) values ($1)";
    const SQL_USER_PREFERENCE: &str = "insert into lean4oj.user_preference (uid) values ($1)";

    let Json(RegisterRequest {
        username,
        identifier,
        email,
        password,
    }) = req?;

    if !check_username(&username) || !check_uid(&identifier) || check_email(&email).is_none() || password.len() != PASSWORD_LENGTH || !password.is_ascii() {
        bad!(BYTES_NULL)
    }

    let mut conn = get_connection().await?;
    let stmt_users = conn.prepare_static(SQL_USERS.into()).await?;
    let stmt_user_information = conn.prepare_static(SQL_USER_INFORMATION.into()).await?;
    let stmt_user_preference = conn.prepare_static(SQL_USER_PREFERENCE.into()).await?;
    let txn = conn.transaction().await?;
    let n = txn.execute(&stmt_users, &[&&*identifier, &&*username, &&*email, &&*password, &now]).await?;
    if n != 1 { return private::err() }
    let n = txn.execute(&stmt_user_information, &[&&*identifier]).await?;
    if n != 1 { return private::err() }
    let n = txn.execute(&stmt_user_preference, &[&&*identifier]).await?;
    if n != 1 { return private::err() }
    txn.commit().await?;

    let session = session::create(identifier.into_string()).await?;
    let encoded = Encoded::try_from(session.id().unwrap_or(Id(0)))?;
    let res = format!(r#"{{"token":"{}"}}"#, Base64Display::new(encoded.as_ref(), &BASE64_STANDARD));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResetPasswordRequest {
    email: CompactString,
    email_verification_code: u32,
    new_password: String,
}

async fn reset_password(
    Extension(now): Extension<SystemTime>,
    req: JsonReqult<ResetPasswordRequest>,
) -> JkmxJsonResponse {
    const SQL: &str = "update lean4oj.users set password = $1 where username != '' and email = $2 returning uid";

    let Json(ResetPasswordRequest {
        email,
        email_verification_code,
        new_password,
    }) = req?;

    if check_email(&email).is_none()
        || new_password.len() != PASSWORD_LENGTH
        || !new_password.is_ascii()
    {
        bad!(BYTES_NULL)
    }

    tokio::time::sleep(DELAY_FOR_SECURITY).await;

    if !email_check(&email, now, email_verification_code) {
        return JkmxJsonResponse::Response(
            StatusCode::OK,
            Bytes::from_static(br#"{"error":"INVALID_EMAIL_VERIFICATION_CODE"}"#),
        );
    }

    let mut conn = get_connection().await?;
    let stmt = conn.prepare_static(SQL.into()).await?;
    let row = conn.query_one(&stmt, &[&new_password, &&*email]).await?;
    let uid = row.try_get(0)?;

    let session = session::reset(uid).await?;
    let encoded = Encoded::try_from(session.id().unwrap_or(Id(0)))?;
    let res = format!(r#"{{"token":"{}"}}"#, Base64Display::new(encoded.as_ref(), &BASE64_STANDARD));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

const fn list_user_sessions(header: &'static Parts) -> RawPayload {
    RawPayload { header, body: br#"{"sessions":[]}"# }
}

pub fn router(header: &'static Parts) -> Router {
    use super::user::{create_api_token, delete_api_token, list_api_tokens};
    Router::new()
        .route("/getSessionInfo", get(get_session_info))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/checkAvailability", get(check_availability))
        .route("/sendEmailVerificationCode", post(send_email))
        .route("/register", post(register))
        .route("/resetPassword", post(reset_password))
        .route("/listUserSessions", post_service(list_user_sessions(header)))
        .route("/createApiToken", post(create_api_token))
        .route("/listApiTokens", post(list_api_tokens))
        .route("/deleteApiToken", post(delete_api_token))
}
