use core::{future::ready, time::Duration};
use std::time::SystemTime;

use axum::{
    Extension, Json, Router,
    extract::Query,
    routing::{get, post, post_service},
};
use base64::{display::Base64Display, prelude::BASE64_STANDARD_NO_PAD};
use bytes::Bytes;
use compact_str::CompactString;
use futures_util::TryStreamExt;
use http::{StatusCode, response::Parts};
use rand::TryRng;
use serde::{Deserialize, Serialize, Serializer, ser::SerializeSeq};
use serde_json::{Serializer as JSerializer, Value};
use smallvec::SmallVec;
use tokio_postgres::{
    Client, IsolationLevel,
    types::{Json as QJson, ToSql},
};
use uuid::{Timestamp, Uuid};

use crate::{
    bad, exs,
    libs::{
        auth::Session_,
        constants::{BYTES_EMPTY, BYTES_NULL, PASSWORD_LENGTH},
        db::{DBError, DBResult, JsonChecked, get_connection},
        lquery, privilege,
        preference::server::Security,
        request::{JsonReqult, RawPayload, Repult},
        response::JkmxJsonResponse,
        serde::WithJson,
        util::{from_millis, get_millis},
        validate::{check_email, check_username},
    },
    models::user::{User, UserA, UserInformation},
};

const NO_SUCH_USER: JkmxJsonResponse = JkmxJsonResponse::Response(
    StatusCode::OK,
    Bytes::from_static(br#"{"error":"NO_SUCH_USER"}"#),
);

mod private {
    use futures_util::FutureExt;

    #[inline]
    pub(super) fn λ(src: &str, shortcut: bool, conn: &mut super::Client) -> impl Future<Output = super::DBResult<bool>> {
        if shortcut {
            core::future::ready(Ok(true)).left_future()
        } else {
            super::privilege::check(src, "Lean4OJ.ManageUser", conn).right_future()
        }
    }

    #[inline]
    pub(super) fn γ(src: &str, dest: &str, conn: &mut super::Client) -> impl Future<Output = super::DBResult<bool>> {
        λ(src, *src == *dest, conn)
    }

    pub(super) fn err() -> super::JkmxJsonResponse {
        let err = super::DBError::new(tokio_postgres::error::Kind::RowCount, Some("database update error".into()));
        return super::JkmxJsonResponse::Error(super::StatusCode::INTERNAL_SERVER_ERROR, err.into());
    }
}

#[derive(Deserialize)]
struct SearchUserRequest {
    query: CompactString,
}

async fn search_user(req: Repult<Query<SearchUserRequest>>) -> JkmxJsonResponse {
    let Query(SearchUserRequest { query }) = req?;

    let Some((dot, query)) = lquery::normalize(&query) else { bad!(BYTES_NULL) };

    let mut conn = get_connection().await?;
    let users = User::search(dot, &query, &mut conn).await?;

    let res = format!(r#"{{"userMetas":{}}}"#, WithJson(users));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetUserMetaRequest {
    uid: CompactString,
    get_privileges: Option<bool>,
}

#[derive(Serialize)]
struct GetUserMetaResponse {
    meta: UserA,
    privileges: privilege::Privileges,
}

async fn get_user_meta(req: JsonReqult<GetUserMetaRequest>) -> JkmxJsonResponse {
    let Json(GetUserMetaRequest { uid, get_privileges }) = req?;

    let mut conn = get_connection().await?;
    let Some(user) = User::by_uid(&uid, &mut conn).await? else { return NO_SUCH_USER };

    let res = if get_privileges == Some(true) {
        let privileges = privilege::all(&user.uid, &mut conn).await?;
        GetUserMetaResponse {
            meta: UserA { user, is_admin: privilege::is_admin(&privileges) },
            privileges,
        }
    } else {
        GetUserMetaResponse {
            meta: UserA { user, is_admin: false },
            privileges: SmallVec::new(),
        }
    };

    JkmxJsonResponse::Response(StatusCode::OK, serde_json::to_vec(&res)?.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateUserProfileRequest {
    user_id: CompactString,
    username: CompactString,
    email: CompactString,
    avatar_info: CompactString,
    nickname: CompactString,
    bio: CompactString,
    information: UserInformation,
}

async fn update_user_profile(
    session: Session_,
    req: JsonReqult<UpdateUserProfileRequest>,
) -> JkmxJsonResponse {
    const SQL_UPDATE_USER: &str = "update lean4oj.users set username = $1, email = $2, avatar_info = $3, nickname = $4, bio = $5 where uid = $6";
    const SQL_UPDATE_INFORMATION: &str = "update lean4oj.user_information set organization = $1, location = $2, url = $3, telegram = $4, qq = $5, github = $6 where uid = $7";

    let Json(UpdateUserProfileRequest { user_id, username, email, avatar_info, nickname, bio, information }) = req?;

    if !check_username(&username) { bad!(BYTES_NULL) }

    let mut conn = get_connection().await?;
    exs!(s_user, session, &mut conn);
    let Some(t_user) = User::by_uid(&user_id, &mut conn).await? else { return NO_SUCH_USER };
    if !private::γ(&s_user.uid, &t_user.uid, &mut conn).await? {
        return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL);
    }

    let stmt_update_user = conn.prepare_static(SQL_UPDATE_USER.into()).await?;
    let stmt_update_information = conn.prepare_static(SQL_UPDATE_INFORMATION.into()).await?;
    let txn = conn.transaction().await?;
    let n = txn.execute(&stmt_update_user, &[&&*username, &&*email, &&*avatar_info, &&*nickname, &&*bio, &&*t_user.uid]).await?;
    if n != 1 { return private::err() }
    let n = txn.execute(&stmt_update_information, &[&&*information.organization, &&*information.location, &&*information.url, &&*information.telegram, &&*information.qq, &&*information.github, &&*t_user.uid]).await?;
    if n != 1 { return private::err() }
    txn.commit().await?;

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetUserListRequest {
    skip_count: u64,
    take_count: u64,
}

async fn get_user_list(req: JsonReqult<GetUserListRequest>) -> JkmxJsonResponse {
    let Json(GetUserListRequest { skip_count, take_count }) = req?;

    let skip = skip_count.min(i64::MAX.cast_unsigned()).cast_signed();
    let take = take_count.min(100).cast_signed();

    let mut conn = get_connection().await?;
    let users = User::list(skip, take, &mut conn).await?;
    let count = User::count(&mut conn).await?;

    let res = format!(r#"{{"userMetas":{},"count":{count}}}"#, WithJson(users));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

const SUBMISSION_COUNT_PER_DAY_COUNT: usize = 53 * 7;

#[derive(Deserialize)]
struct GetUserDetailRequest {
    uid: CompactString,
    timezone: CompactString,
    now: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GetUserDetailResponse {
    meta: User,
    information: UserInformation,
    submission_count_per_day: Box<[u32]>,
    rank: u64,
    hasPrivilege: bool,
}

#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
async fn get_user_detail(req: JsonReqult<GetUserDetailRequest>) -> JkmxJsonResponse {
    const SQL_RANK: &str = "select count(*) from lean4oj.users where ac > $1";
    const SQL_PER_DAY: &str = "select (($1::timestamp at time zone 'UTC' at time zone $2)::date - (submit_time at time zone 'UTC' at time zone $2)::date) as d, count(*) from lean4oj.submissions where submitter = $3 and submit_time between ((($1::timestamp at time zone 'UTC' at time zone $2)::date - $4::integer)::timestamp at time zone $2 at time zone 'UTC') and (($1::timestamp at time zone 'UTC' at time zone $2)::date + 1)::timestamp at time zone $2 at time zone 'UTC' group by d";

    let Json(GetUserDetailRequest { uid, timezone, now }) = req?;
    let now = from_millis(now);

    let mut conn = get_connection().await?;
    let Some(user) = User::by_uid(&uid, &mut conn).await? else { return NO_SUCH_USER };
    let stmt = conn.prepare_static(SQL_RANK.into()).await?;
    let row = conn.query_one(&stmt, &[&user.ac.cast_signed()]).await?;
    let information = UserInformation::of(&uid, &mut conn).await?;

    let mut submission_count_per_day = unsafe { Box::new_zeroed_slice(SUBMISSION_COUNT_PER_DAY_COUNT).assume_init() };

    let stmt = conn.prepare_static(SQL_PER_DAY.into()).await?;
    let params: [&(dyn ToSql + Sync); 4] = [&now, &&*timezone, &&*uid, &(SUBMISSION_COUNT_PER_DAY_COUNT as i32 - 1)];
    let stream = conn.query_raw(&stmt, params).await?;
    stream.try_for_each(|row| ready(try {
        let d = row.try_get::<_, i32>(0)?;
        let c = row.try_get::<_, i64>(1)?;
        if let Some(r) = submission_count_per_day.get_mut(SUBMISSION_COUNT_PER_DAY_COUNT - 1 - d as usize) {
            *r = c as u32;
        }
    })).await?;

    let res = GetUserDetailResponse {
        meta: user,
        information,
        submission_count_per_day,
        rank: row.try_get::<_, i64>(0)?.cast_unsigned() + 1,
        hasPrivilege: true,
    };

    JkmxJsonResponse::Response(StatusCode::OK, serde_json::to_vec(&res)?.into())
}

#[derive(Deserialize)]
pub struct GetSingleUserRequest {
    uid: CompactString,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GetUserProfileResponse {
    meta: User,
    information: UserInformation,
    public_email: bool,
    avatar_info: CompactString,
}

async fn get_user_profile(req: JsonReqult<GetSingleUserRequest>) -> JkmxJsonResponse {
    let Json(GetSingleUserRequest { uid }) = req?;

    let mut conn = get_connection().await?;
    let Some(user) = User::by_uid(&uid, &mut conn).await? else { return NO_SUCH_USER };
    let information = UserInformation::of(&uid, &mut conn).await?;

    let avatar_info = user.avatar_info.clone();
    let res = GetUserProfileResponse {
        meta: user,
        information,
        public_email: true,
        avatar_info,
    };

    JkmxJsonResponse::Response(StatusCode::OK, serde_json::to_vec(&res)?.into())
}

async fn get_user_preference(
    session: Session_,
    req: JsonReqult<GetSingleUserRequest>,
) -> JkmxJsonResponse {
    const SQL_GET_PREF: &str = "select preference from lean4oj.user_preference where uid = $1";

    let Json(GetSingleUserRequest { uid }) = req?;

    let mut conn = get_connection().await?;
    exs!(s_user, session, &mut conn);
    let Some(t_user) = User::by_uid(&uid, &mut conn).await? else { return NO_SUCH_USER };
    if !private::γ(&s_user.uid, &t_user.uid, &mut conn).await? {
        return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL);
    }

    let stmt = conn.prepare_static(SQL_GET_PREF.into()).await?;
    let row = conn.query_one(&stmt, &[&&*t_user.uid]).await?;
    let pref = row.try_get::<_, JsonChecked>(0)?;

    let res = format!(r#"{{"meta":{},"preference":{}}}"#, WithJson(t_user), unsafe { core::str::from_utf8_unchecked(pref.0) });
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateUserPreferenceRequest {
    user_id: CompactString,
    preference: serde_json::Map<String, Value>,
}

async fn update_user_preference(
    session: Session_,
    req: JsonReqult<UpdateUserPreferenceRequest>,
) -> JkmxJsonResponse {
    const SQL: &str = "update lean4oj.user_preference set preference = $1 where uid = $2";

    let Json(UpdateUserPreferenceRequest { user_id, preference }) = req?;

    let mut conn = get_connection().await?;
    exs!(s_user, session, &mut conn);
    let Some(t_user) = User::by_uid(&user_id, &mut conn).await? else { return NO_SUCH_USER };
    if !private::γ(&s_user.uid, &t_user.uid, &mut conn).await? {
        return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL);
    }

    let stmt = conn.prepare_static(SQL.into()).await?;
    let n = conn.execute(&stmt, &[&QJson(preference), &&*t_user.uid]).await?;
    if n != 1 { return private::err() }

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

async fn get_user_security_settings(req: JsonReqult<GetSingleUserRequest>) -> JkmxJsonResponse {
    let Json(GetSingleUserRequest { uid }) = req?;

    let mut conn = get_connection().await?;
    let Some(user) = User::by_uid(&uid, &mut conn).await? else { return NO_SUCH_USER };

    let res = format!(r#"{{"meta":{}}}"#, WithJson(user));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

const fn query_audit_logs(header: &'static Parts) -> RawPayload {
    RawPayload { header, body: br#"{"count":0,"results":[]}"# }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdatePasswordRequest {
    user_id: CompactString,
    old_password: Option<CompactString>,
    password: CompactString,
}

async fn update_password(
    session: Session_,
    req: JsonReqult<UpdatePasswordRequest>,
) -> JkmxJsonResponse {
    const SQL: &str = "update lean4oj.users set password = $1 where uid = $2";

    let Json(UpdatePasswordRequest { user_id, old_password, password }) = req?;

    if password.len() != PASSWORD_LENGTH || !password.is_ascii() { bad!(BYTES_NULL) }

    let mut conn = get_connection().await?;
    exs!(s_user, session, &mut conn);
    let Some(t_user) = User::by_uid(&user_id, &mut conn).await? else { return NO_SUCH_USER };
    if !private::λ(
        &s_user.uid,
        *s_user.uid == *t_user.uid && old_password.is_some_and(|p| *p.as_bytes() == t_user.password),
        &mut conn,
    ).await? {
        return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL);
    }

    let stmt = conn.prepare_static(SQL.into()).await?;
    let n = conn.execute(&stmt, &[&&*password, &&*t_user.uid]).await?;
    if n != 1 { return private::err() }

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

#[derive(Deserialize)]
struct UpdateEmailRequest {
    email: CompactString,
}

async fn update_email(
    session: Session_,
    req: JsonReqult<UpdateEmailRequest>,
) -> JkmxJsonResponse {
    const SQL: &str = "update lean4oj.users set email = $1 where uid = $2";

    let Json(UpdateEmailRequest { email }) = req?;

    if check_email(&email).is_none() { bad!(BYTES_NULL) }

    let mut conn = get_connection().await?;
    exs!(user, session, &mut conn);

    let stmt = conn.prepare_static(SQL.into()).await?;
    let n = conn.execute(&stmt, &[&&*email, &&*user.uid]).await?;
    if n != 1 { return private::err() }

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

#[derive(Deserialize)]
pub struct CreateApiTokenRequest {
    name: CompactString,
    uid: CompactString,
}

pub async fn create_api_token(
    Extension(now): Extension<SystemTime>,
    session: Session_,
    req: JsonReqult<CreateApiTokenRequest>,
) -> JkmxJsonResponse {
    const SQL_PRE: &str = "select count(*) from lean4oj.user_api_tokens where uid = $1";
    const SQL: &str = "insert into lean4oj.user_api_tokens (id, uid, token, name, created_at) values ($1, $2, $3, $4, $5)";

    let Json(CreateApiTokenRequest { name, uid }) = req?;

    let mut conn = get_connection().await?;
    exs!(s_user, session, &mut conn);
    let Some(t_user) = User::by_uid(&uid, &mut conn).await? else { return NO_SUCH_USER };
    if !private::γ(&s_user.uid, &t_user.uid, &mut conn).await? {
        return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL);
    }

    let time = unsafe { core::mem::transmute::<SystemTime, Duration>(now) };
    let timestamp = Timestamp::from_unix(uuid::timestamp::context::shared_context_v7(), time.as_secs(), time.subsec_nanos());
    let uuid = Uuid::new_v7(timestamp);
    let mut token: [u8; 64] = [0; 64];
    rand::rngs::SysRng.try_fill_bytes(&mut token)?;

    let stmt_pre = conn.prepare_static(SQL_PRE.into()).await?;
    let stmt = conn.prepare_static(SQL.into()).await?;

    let txn = conn
        .build_transaction()
        .isolation_level(IsolationLevel::Serializable)
        .start()
        .await?;
    let row = txn.query_one(&stmt_pre, &[&&*t_user.uid]).await?;
    let n = row.try_get::<_, i64>(0)?.cast_unsigned() as usize;
    if n >= const { Security::default().max_api_tokens } {
        return JkmxJsonResponse::Response(
            StatusCode::OK,
            Bytes::from_static(br#"{"error":"TOO_MANY_TOKENS"}"#),
        );
    }
    let n = txn.execute(&stmt, &[&uuid, &&*t_user.uid, &token.as_slice(), &&*name, &now]).await?;
    if n != 1 { return private::err() }
    txn.commit().await?;

    let res = format!(
        r#"{{"token":"l4oj-{}","tokenUUID":"{uuid}","name":{},"createdAt":{}}}"#,
        Base64Display::new(&token, &BASE64_STANDARD_NO_PAD), WithJson(&*name), time.as_millis(),
    );
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TokenInfo {
    id: Uuid,
    name: CompactString,
    created_at: u128,
    last_used_at: Option<u128>,
}

pub async fn list_api_tokens(
    session: Session_,
    req: JsonReqult<GetSingleUserRequest>,
) -> JkmxJsonResponse {
    const SQL: &str = "select id, name, created_at, last_used_at from lean4oj.user_api_tokens where uid = $1 order by coalesce(last_used_at, created_at) desc";

    let Json(GetSingleUserRequest { uid }) = req?;

    let mut conn = get_connection().await?;
    exs!(s_user, session, &mut conn);
    let Some(t_user) = User::by_uid(&uid, &mut conn).await? else { return NO_SUCH_USER };
    if !private::γ(&s_user.uid, &t_user.uid, &mut conn).await? {
        return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL);
    }

    let stmt = conn.prepare_static(SQL.into()).await?;
    let stream = conn.query_raw(&stmt, &[&&*t_user.uid]).await?;

    let mut res = r#"{"tokens":"#.to_owned();
    let mut ser = JSerializer::new(unsafe { res.as_mut_vec() });
    let mut seq = ser.serialize_seq(None)?;
    stream.try_for_each(|row| ready(try {
        let id = row.try_get(0)?;
        let name = row.try_get::<_, &str>(1)?.into();
        let created_at = get_millis(row.try_get(2)?);
        let last_used_at = row.try_get::<_, Option<SystemTime>>(3)?.map(get_millis);

        let _ = seq.serialize_element(&TokenInfo { id, name, created_at, last_used_at });
    })).await?;
    seq.end()?;
    res.push('}');
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
pub struct DeleteApiTokenRequest {
    #[serde(rename = "tokenUUID")]
    token_uuid: Uuid,
    uid: CompactString,
}

pub async fn delete_api_token(
    session: Session_,
    req: JsonReqult<DeleteApiTokenRequest>,
) -> JkmxJsonResponse {
    const SQL: &str = "delete from lean4oj.user_api_tokens where id = $1 and uid = $2";

    let Json(DeleteApiTokenRequest { token_uuid, uid }) = req?;

    let mut conn = get_connection().await?;
    exs!(s_user, session, &mut conn);
    let Some(t_user) = User::by_uid(&uid, &mut conn).await? else { return NO_SUCH_USER };
    if !private::γ(&s_user.uid, &t_user.uid, &mut conn).await? {
        return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL);
    }

    let stmt = conn.prepare_static(SQL.into()).await?;
    let n = conn.execute(&stmt, &[&&token_uuid, &&*t_user.uid]).await?;
    if n != 1 {
        return JkmxJsonResponse::Response(
            StatusCode::OK,
            Bytes::from_static(br#"{"error":"NO_SUCH_TOKEN"}"#),
        );
    }

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

pub fn router(header: &'static Parts) -> Router {
    Router::new()
        .route("/searchUser", get(search_user))
        .route("/getUserMeta", post(get_user_meta))
        .route("/updateUserProfile", post(update_user_profile))
        .route("/getUserList", post(get_user_list))
        .route("/getUserDetail", post(get_user_detail))
        .route("/getUserProfile", post(get_user_profile))
        .route("/getUserPreference", post(get_user_preference))
        .route("/updateUserPreference", post(update_user_preference))
        .route("/getUserSecuritySettings", post(get_user_security_settings))
        .route("/queryAuditLogs", post_service(query_audit_logs(header)))
        .route("/updateUserPassword", post(update_password))
        .route("/updateUserSelfEmail", post(update_email))
}
