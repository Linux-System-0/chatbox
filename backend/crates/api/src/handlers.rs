//! HTTP handlers.

use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::header;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chatbox_core::error::Error;
use chatbox_core::model::{GroupRole, MessageKind, Platform, SearchQuery, SortOrder};
use chatbox_core::services::{AuthContext, LoginInput, RegisterInput, SendRequest};
use chatbox_plugin::{HookContext, HookOutcome};
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};
use crate::AppState;

/// An authenticated caller extracted from the `Authorization` header.
pub struct AuthUser(pub AuthContext);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or(Error::Unauthorized)?;
        let token = header.strip_prefix("Bearer ").ok_or(Error::Unauthorized)?;
        let ctx = state.services.authenticate(token).await?;
        Ok(AuthUser(ctx))
    }
}

async fn rate_limit(state: &AppState, key: String, limit: u32, secs: u64) -> ApiResult<()> {
    let allowed = state
        .services
        .infra
        .rate_limiter
        .check(&key, limit, Duration::from_secs(secs))
        .await?;
    if !allowed {
        return Err(Error::RateLimited.into());
    }
    Ok(())
}

fn platform_of(value: Option<&str>) -> Platform {
    match value {
        Some("web") => Platform::Web,
        Some("android") => Platform::Android,
        _ => Platform::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

/// Liveness probe.
pub async fn health_live() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok" })))
}

/// Readiness probe.
pub async fn health_ready(State(state): State<AppState>) -> impl IntoResponse {
    let _ = &state;
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "ready" })),
    )
}

// ---------------------------------------------------------------------------
// Authentication
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RegisterBody {
    user_id: String,
    password: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    device_name: Option<String>,
    #[serde(default)]
    platform: Option<String>,
}

/// `POST /auth/register`
pub async fn register(
    State(state): State<AppState>,
    Json(body): Json<RegisterBody>,
) -> ApiResult<Json<serde_json::Value>> {
    rate_limit(&state, format!("register:{}", body.user_id), 10, 60).await?;
    let (user, tokens) = state
        .services
        .register(RegisterInput {
            user_id: body.user_id,
            password: body.password,
            display_name: body.display_name,
            device_name: body.device_name.unwrap_or_default(),
            platform: platform_of(body.platform.as_deref()),
        })
        .await?;
    let hook = HookContext::new(
        "user.register",
        serde_json::json!({ "user_id": user.id, "handle": user.user_id }),
    );
    let _ = state.plugins.on_user_register(&hook).await;
    Ok(Json(serde_json::json!({ "user": user, "tokens": tokens })))
}

#[derive(Deserialize)]
pub struct LoginBody {
    user_id: String,
    password: String,
    #[serde(default)]
    device_name: Option<String>,
    #[serde(default)]
    platform: Option<String>,
}

/// `POST /auth/login`
pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginBody>,
) -> ApiResult<Json<serde_json::Value>> {
    rate_limit(&state, format!("login:{}", body.user_id), 20, 60).await?;
    let (user, tokens) = state
        .services
        .login(LoginInput {
            user_id: body.user_id,
            password: body.password,
            device_name: body.device_name.unwrap_or_default(),
            platform: platform_of(body.platform.as_deref()),
        })
        .await?;
    Ok(Json(serde_json::json!({ "user": user, "tokens": tokens })))
}

#[derive(Deserialize)]
pub struct RefreshBody {
    refresh_token: String,
}

/// `POST /auth/refresh`
pub async fn refresh(
    State(state): State<AppState>,
    Json(body): Json<RefreshBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let tokens = state.services.refresh(&body.refresh_token).await?;
    Ok(Json(serde_json::json!({ "tokens": tokens })))
}

/// `POST /auth/logout`
pub async fn logout(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
) -> ApiResult<StatusCode> {
    state.services.logout(&ctx.session_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /auth/sessions`
pub async fn sessions(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let sessions = state.services.list_sessions(ctx.user_id).await?;
    Ok(Json(serde_json::json!({ "sessions": sessions })))
}

// ---------------------------------------------------------------------------
// Users and devices
// ---------------------------------------------------------------------------

/// `GET /users/me`
pub async fn me(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let user = state.services.get_user(ctx.user_id).await?;
    Ok(Json(serde_json::json!({ "user": user })))
}

#[derive(Deserialize)]
pub struct UpdateMeBody {
    #[serde(default)]
    display_name: Option<String>,
}

/// `PATCH /users/me`
pub async fn update_me(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Json(body): Json<UpdateMeBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let user = state
        .services
        .update_profile(&ctx, body.display_name)
        .await?;
    Ok(Json(serde_json::json!({ "user": user })))
}

/// `GET /users/{user_id}`
pub async fn get_user(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(user_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let user = state.services.get_user_by_user_id(&user_id).await?;
    Ok(Json(serde_json::json!({ "user": user })))
}

/// `GET /devices`
pub async fn list_devices(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let devices = state.services.list_devices(ctx.user_id).await?;
    Ok(Json(serde_json::json!({ "devices": devices })))
}

/// `DELETE /devices/{id}`
pub async fn delete_device(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    state.services.delete_device(&ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Contacts
// ---------------------------------------------------------------------------

/// `GET /contacts`
pub async fn list_contacts(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let contacts = state.services.list_contacts(&ctx).await?;
    Ok(Json(serde_json::json!({ "contacts": contacts })))
}

#[derive(Deserialize)]
pub struct RemarkBody {
    #[serde(default)]
    remark: Option<String>,
}

/// `PATCH /contacts/{user_id}`
pub async fn set_remark(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(user_id): Path<String>,
    Json(body): Json<RemarkBody>,
) -> ApiResult<StatusCode> {
    state
        .services
        .set_contact_remark(&ctx, &user_id, body.remark)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /contacts/{user_id}`
pub async fn remove_contact(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(user_id): Path<String>,
) -> ApiResult<StatusCode> {
    state.services.remove_contact(&ctx, &user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct FriendRequestBody {
    user_id: String,
    #[serde(default)]
    message: Option<String>,
}

/// `POST /contacts/requests`
pub async fn send_request(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Json(body): Json<FriendRequestBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let request = state
        .services
        .send_friend_request(&ctx, &body.user_id, body.message)
        .await?;
    Ok(Json(serde_json::json!({ "request": request })))
}

/// `GET /contacts/requests`
pub async fn list_requests(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let requests = state.services.list_friend_requests(&ctx).await?;
    Ok(Json(serde_json::json!({ "requests": requests })))
}

/// `POST /contacts/requests/{id}/accept`
pub async fn accept_request(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    let request = state.services.accept_friend_request(&ctx, id).await?;
    Ok(Json(serde_json::json!({ "request": request })))
}

/// `POST /contacts/requests/{id}/reject`
pub async fn reject_request(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    let request = state.services.reject_friend_request(&ctx, id).await?;
    Ok(Json(serde_json::json!({ "request": request })))
}

// ---------------------------------------------------------------------------
// Conversations and messages
// ---------------------------------------------------------------------------

/// `GET /conversations`
pub async fn list_conversations(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let conversations = state.services.list_conversations(&ctx).await?;
    Ok(Json(serde_json::json!({ "conversations": conversations })))
}

#[derive(Deserialize)]
pub struct DirectBody {
    user_id: String,
}

/// `POST /conversations/direct`
pub async fn create_direct(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Json(body): Json<DirectBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let conversation = state
        .services
        .get_or_create_direct(&ctx, &body.user_id)
        .await?;
    Ok(Json(serde_json::json!({ "conversation": conversation })))
}

#[derive(Deserialize)]
pub struct MessageQuery {
    #[serde(default)]
    before: Option<u64>,
    #[serde(default)]
    after: Option<u64>,
    #[serde(default)]
    limit: Option<u32>,
}

/// `GET /conversations/{id}/messages`
pub async fn list_messages(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(id): Path<i64>,
    Query(query): Query<MessageQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let messages = state
        .services
        .list_messages(&ctx, id, query.before, query.after, query.limit)
        .await?;
    Ok(Json(serde_json::json!({ "messages": messages })))
}

#[derive(Deserialize)]
pub struct SendMessageBody {
    client_msg_id: String,
    kind: MessageKind,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    attachment_id: Option<i64>,
}

/// `POST /conversations/{id}/messages`
pub async fn send_message(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<SendMessageBody>,
) -> ApiResult<Json<serde_json::Value>> {
    rate_limit(&state, format!("send:{}", ctx.user_id), 120, 60).await?;
    let hook = HookContext::new(
        "message.before_persist",
        serde_json::json!({
            "sender_id": ctx.user_id,
            "conversation_id": id,
            "kind": body.kind,
            "content": body.content,
        }),
    );
    if let HookOutcome::Reject(reason) = state.plugins.on_message_before_persist(&hook).await? {
        return Err(Error::Forbidden(reason).into());
    }
    let message = state
        .services
        .send_message(
            &ctx,
            SendRequest {
                conversation_id: id,
                client_msg_id: body.client_msg_id,
                kind: body.kind,
                content: body.content,
                attachment_id: body.attachment_id,
            },
        )
        .await?;
    let after = HookContext::new(
        "message.after_persist",
        serde_json::to_value(&message).unwrap_or_default(),
    );
    let _ = state.plugins.on_message_after_persist(&after).await;
    Ok(Json(serde_json::json!({ "message": message })))
}

#[derive(Deserialize)]
pub struct ReadBody {
    seq: u64,
}

/// `POST /conversations/{id}/read`
pub async fn mark_read(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<ReadBody>,
) -> ApiResult<StatusCode> {
    state.services.mark_read(&ctx, id, body.seq).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateGroupBody {
    name: String,
    #[serde(default)]
    members: Vec<String>,
}

/// `POST /groups`
pub async fn create_group(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Json(body): Json<CreateGroupBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let group = state
        .services
        .create_group(&ctx, &body.name, &body.members)
        .await?;
    let hook = HookContext::new(
        "group.created",
        serde_json::json!({ "group_id": group.group.id, "owner_id": group.group.owner_id }),
    );
    let _ = state.plugins.on_group_created(&hook).await;
    Ok(Json(serde_json::json!({ "group": group })))
}

/// `GET /groups/{id}`
pub async fn get_group(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    let group = state.services.get_group(&ctx, id).await?;
    Ok(Json(serde_json::json!({ "group": group })))
}

#[derive(Deserialize)]
pub struct RenameGroupBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    invite_policy: Option<GroupRole>,
}

/// `PATCH /groups/{id}`
pub async fn rename_group(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<RenameGroupBody>,
) -> ApiResult<Json<serde_json::Value>> {
    if let Some(name) = body.name {
        state.services.rename_group(&ctx, id, &name).await?;
    }
    if let Some(policy) = body.invite_policy {
        state.services.set_invite_policy(&ctx, id, policy).await?;
    }
    let group = state.services.get_group(&ctx, id).await?;
    Ok(Json(serde_json::json!({ "group": group })))
}

#[derive(Deserialize)]
pub struct MembersBody {
    members: Vec<String>,
}

/// `POST /groups/{id}/members`
pub async fn add_members(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<MembersBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let group = state
        .services
        .add_group_members(&ctx, id, &body.members)
        .await?;
    Ok(Json(serde_json::json!({ "group": group })))
}

/// `DELETE /groups/{id}/members/{user_id}`
pub async fn kick_member(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path((id, user_id)): Path<(i64, String)>,
) -> ApiResult<StatusCode> {
    state.services.kick_group_member(&ctx, id, &user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct MuteBody {
    #[serde(default)]
    minutes: Option<i64>,
}

/// `POST /groups/{id}/members/{user_id}/mute`
pub async fn mute_member(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path((id, user_id)): Path<(i64, String)>,
    Json(body): Json<MuteBody>,
) -> ApiResult<StatusCode> {
    let until = body
        .minutes
        .map(|m| state.services.now() + chrono::Duration::minutes(m));
    state
        .services
        .mute_group_member(&ctx, id, &user_id, until)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct TransferBody {
    user_id: String,
}

/// `POST /groups/{id}/transfer`
pub async fn transfer(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<TransferBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let group = state
        .services
        .transfer_ownership(&ctx, id, &body.user_id)
        .await?;
    Ok(Json(serde_json::json!({ "group": group })))
}

/// `POST /groups/{id}/leave`
pub async fn leave_group(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    state.services.leave_group(&ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /groups/{id}`
pub async fn dissolve_group(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    state.services.dissolve_group(&ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Blocks
// ---------------------------------------------------------------------------

/// `GET /blocks`
pub async fn list_blocks(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let blocks = state.services.list_blocked_users(&ctx).await?;
    Ok(Json(serde_json::json!({ "blocks": blocks })))
}

/// `POST /blocks/{user_id}`
pub async fn block_user(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(user_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let block = state.services.block_user(&ctx, &user_id).await?;
    Ok(Json(serde_json::json!({ "block": block })))
}

/// `DELETE /blocks/{user_id}`
pub async fn unblock_user(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(user_id): Path<String>,
) -> ApiResult<StatusCode> {
    state.services.unblock_user(&ctx, &user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SearchParams {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    filename: Option<String>,
    #[serde(default)]
    conversation_id: Option<i64>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    sort: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

/// `GET /search`
pub async fn search(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Query(params): Query<SearchParams>,
) -> ApiResult<Json<serde_json::Value>> {
    let kind = match params.kind.as_deref() {
        Some("text") => Some(MessageKind::Text),
        Some("file") => Some(MessageKind::File),
        Some("system") => Some(MessageKind::System),
        Some(_) => return Err(Error::Invalid("invalid kind".into()).into()),
        None => None,
    };
    let parse_time =
        |value: Option<String>| -> Result<Option<chrono::DateTime<chrono::Utc>>, Error> {
            value
                .map(|v| {
                    chrono::DateTime::parse_from_rfc3339(&v)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .map_err(|_| Error::Invalid("invalid timestamp".into()))
                })
                .transpose()
        };
    let sort = match params.sort.as_deref() {
        Some("asc") => SortOrder::Asc,
        Some("desc") | None => SortOrder::Desc,
        Some(_) => return Err(Error::Invalid("invalid sort".into()).into()),
    };
    let query = SearchQuery {
        viewer_id: ctx.user_id,
        conversation_id: params.conversation_id,
        kind,
        content: params.q,
        filename: params.filename,
        from: parse_time(params.from)?,
        to: parse_time(params.to)?,
        sort,
        limit: params.limit.unwrap_or(50),
    };
    let results = state.services.search(&ctx, &query).await?;
    Ok(Json(serde_json::json!({ "results": results })))
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct PresignBody {
    filename: String,
    content_type: String,
    size: u64,
}

/// `POST /files/presign`
pub async fn presign(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Json(body): Json<PresignBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let upload = state
        .services
        .presign_attachment(&ctx, &body.filename, &body.content_type, body.size)
        .await?;
    Ok(Json(serde_json::json!({ "upload": upload })))
}

/// `PUT /files/upload/{id}` — used by clients when the storage backend does
/// not accept direct uploads (local development).
pub async fn upload(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(id): Path<i64>,
    body: Bytes,
) -> ApiResult<StatusCode> {
    state.services.store_upload(&ctx, id, &body).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct CompleteBody {
    filename: String,
    content_type: String,
}

/// `POST /files/{id}/complete`
pub async fn complete_file(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<CompleteBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let attachment = state
        .services
        .complete_attachment(&ctx, id, &body.filename, &body.content_type)
        .await?;
    Ok(Json(serde_json::json!({ "attachment": attachment })))
}

/// `GET /files/{id}`
pub async fn get_file(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    let (attachment, url) = state.services.get_attachment(&ctx, id).await?;
    Ok(Json(
        serde_json::json!({ "attachment": attachment, "url": url }),
    ))
}

/// `GET /files/{id}/content`
pub async fn file_content(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<impl IntoResponse> {
    let attachment = state
        .services
        .stores
        .attachments
        .get(id)
        .await?
        .ok_or(Error::NotFound)?;
    let _ = ctx;
    let bytes = state
        .services
        .infra
        .objects
        .get(&attachment.storage_key)
        .await?
        .ok_or(Error::NotFound)?;
    let mut headers = header::HeaderMap::new();
    if let Ok(value) = header::HeaderValue::from_str(&attachment.mime) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    Ok((headers, bytes))
}

// ---------------------------------------------------------------------------
// Plugins
// ---------------------------------------------------------------------------

/// `GET /plugins/client`
pub async fn client_plugins(State(state): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let manifests = state.plugins.client_manifests().await;
    Ok(Json(serde_json::json!({ "plugins": manifests })))
}

/// `GET /plugins/server`
pub async fn server_plugins(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    if !state.is_admin(&user_handle(&state, &ctx).await?) {
        return Err(Error::Forbidden("admin only".into()).into());
    }
    let plugins = state.plugins.list().await;
    Ok(Json(serde_json::json!({ "plugins": plugins })))
}

/// `POST /plugins/server/{name}/enable`
pub async fn enable_plugin(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(name): Path<String>,
) -> ApiResult<StatusCode> {
    if !state.is_admin(&user_handle(&state, &ctx).await?) {
        return Err(Error::Forbidden("admin only".into()).into());
    }
    state.plugins.enable(&name).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /plugins/server/{name}/disable`
pub async fn disable_plugin(
    State(state): State<AppState>,
    AuthUser(ctx): AuthUser,
    Path(name): Path<String>,
) -> ApiResult<StatusCode> {
    if !state.is_admin(&user_handle(&state, &ctx).await?) {
        return Err(Error::Forbidden("admin only".into()).into());
    }
    state.plugins.disable(&name).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn user_handle(state: &AppState, ctx: &AuthContext) -> ApiResult<String> {
    let user = state.services.get_user(ctx.user_id).await?;
    Ok(user.user_id)
}

// Keep `Serialize` referenced for response types defined by callers.
#[allow(dead_code)]
fn _assert_serialize<T: Serialize>() {}
