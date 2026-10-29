//! Loopback-only, authenticated control plane. Audio mutations use the same
//! command handlers, profile locks and persistence as the desktop UI.
mod osd;
mod policy;
mod registry;

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    sync::{Arc, LazyLock},
};
use tauri::{Emitter, Manager};
use tokio::sync::Semaphore;

static COMMANDS: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("commands.json")).expect("compiled API catalog")
});

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}
impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
    fn invalid(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_params", message)
    }
    fn command(message: String) -> Self {
        Self::new(StatusCode::CONFLICT, "command_failed", message)
    }
    fn internal(message: String) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
    }
    fn unknown_method() -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "unknown_method",
            "Unknown API method",
        )
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({"ok": false, "error": {"code": self.code, "message": self.message}})),
        )
            .into_response()
    }
}
fn arg<T: DeserializeOwned>(params: &Value, key: &str) -> Result<T, ApiError> {
    serde_json::from_value(params.get(key).cloned().unwrap_or(Value::Null))
        .map_err(|e| ApiError::invalid(format!("{key}: {e}")))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    method: String,
    #[serde(default = "empty_params")]
    params: Value,
}
fn empty_params() -> Value {
    json!({})
}

fn validate(request: &Request) -> Result<bool, ApiError> {
    let entry = COMMANDS
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["method"] == request.method)
        .ok_or_else(ApiError::unknown_method)?;
    let params = request
        .params
        .as_object()
        .ok_or_else(|| ApiError::invalid("params must be an object"))?;
    let fields = entry["params"].as_array().unwrap();
    for key in params.keys() {
        if !fields.iter().any(|field| field["name"] == *key) {
            return Err(ApiError::invalid(format!("unknown parameter: {key}")));
        }
    }
    for field in fields {
        let name = field["name"].as_str().unwrap();
        if field["required"] == true && (!params.contains_key(name) || params[name].is_null()) {
            return Err(ApiError::invalid(format!("missing parameter: {name}")));
        }
    }
    Ok(entry["read_only"] == true)
}

type Dispatcher = dyn Fn(&str, Value) -> Result<Value, ApiError> + Send + Sync;
#[derive(Clone)]
struct ApiState {
    token: Arc<String>,
    host: Arc<String>,
    gate: Arc<Semaphore>,
    dispatch: Arc<Dispatcher>,
}

fn authorize(headers: &HeaderMap, state: &ApiState) -> Result<(), ApiError> {
    // Browser requests are deliberately unsupported, including opaque origins.
    if headers.contains_key("origin") {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "browser_origin",
            "Browser origins are not allowed",
        ));
    }
    if headers.get("host").and_then(|v| v.to_str().ok()) != Some(state.host.as_str()) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "invalid_host",
            "Use the discovered 127.0.0.1 endpoint",
        ));
    }
    let expected = format!("Bearer {}", state.token);
    let actual = headers
        .get("authorization")
        .map(|h| h.as_bytes())
        .unwrap_or_default();
    let difference = actual
        .iter()
        .zip(expected.as_bytes())
        .fold(0u8, |a, (b, c)| a | (b ^ c));
    if actual.len() != expected.len() || difference != 0 {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "A valid Bearer token is required",
        ));
    }
    Ok(())
}
async fn health(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    authorize(&headers, &state)?;
    Ok(Json(
        json!({"ok": true, "result": {"api_version": 1, "application": "Mixweave", "version": env!("CARGO_PKG_VERSION")}}),
    ))
}
async fn catalog(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    authorize(&headers, &state)?;
    Ok(Json(json!({"ok": true, "result": *COMMANDS})))
}
async fn rpc(
    State(state): State<ApiState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    authorize(&headers, &state)?;
    if !headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(';').next() == Some("application/json"))
    {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "content_type",
            "Use application/json",
        ));
    }
    let request: Request =
        serde_json::from_slice(&body).map_err(|e| ApiError::invalid(e.to_string()))?;
    validate(&request)?;
    // No unbounded queue of writes. Commands complete on a blocking worker,
    // never on the GTK main thread. The permit survives client disconnection.
    let permit = state.gate.clone().try_acquire_owned().map_err(|_| {
        ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "busy",
            "Another API command is running; retry after completion",
        )
    })?;
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        (state.dispatch)(&request.method, request.params)
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))??;
    Ok(Json(json!({"ok": true, "result": result})))
}
fn router(state: ApiState) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/commands", get(catalog))
        .route("/v1/rpc", post(rpc))
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .with_state(state)
}

/// Handle of the running server; `None` while the API is stopped.
static SERVER: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>> =
    std::sync::Mutex::new(None);

fn discovery_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|dir| dir.join("mixweave").join("local-api.json"))
}

/// Start the API unless the user turned it off in Settings or the
/// environment disables it. An unavailable API must not prevent normal use
/// of the mixer. Discovery is atomically replaced, mode 0600.
pub fn start(app: tauri::AppHandle) {
    if std::env::var("MIXWEAVE_API_DISABLED").as_deref() == Ok("1") {
        return;
    }
    let enabled = app
        .state::<crate::state::AppState>()
        .lock_mixer()
        .map(|mixer| mixer.prefs.local_api_enabled)
        .unwrap_or(true);
    if !enabled {
        return;
    }
    let mut server = SERVER.lock().unwrap_or_else(|e| e.into_inner());
    if server.is_some() {
        return;
    }
    let (shutdown, signal) = tokio::sync::oneshot::channel();
    match start_inner(app, signal) {
        Ok(()) => *server = Some(shutdown),
        Err(error) => eprintln!("mixweave: local API unavailable: {error}"),
    }
}

/// Stop the server, close its listener and remove the discovery file.
pub fn stop() {
    let shutdown = SERVER.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(shutdown) = shutdown {
        let _ = shutdown.send(());
        if let Some(path) = discovery_path() {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn start_inner(
    app: tauri::AppHandle,
    shutdown: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let port: u16 = std::env::var("MIXWEAVE_API_PORT")
        .unwrap_or_else(|_| "0".into())
        .parse()?;
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let mut random = [0u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
    let token: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let directory = dirs::config_dir()
        .ok_or("no config directory")?
        .join("mixweave");
    std::fs::create_dir_all(&directory)?;
    let mut discovery = tempfile::NamedTempFile::new_in(&directory)?;
    discovery
        .as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    serde_json::to_writer(
        &mut discovery,
        &json!({"url": format!("http://{address}"), "token": token, "pid": std::process::id(), "api_version": 1}),
    )?;
    discovery.flush()?;
    discovery.persist(directory.join("local-api.json"))?;
    let dispatch = Arc::new(move |method: &str, mut params: Value| {
        let read_only = validate(&Request {
            method: method.into(),
            params: params.clone(),
        })?;
        {
            let state = app.state::<crate::state::AppState>();
            let mixer = state.lock_mixer().map_err(ApiError::internal)?;
            policy::clamp_volume(&mixer, method, &mut params);
            policy::validate(&mixer, method, &params)?;
        }
        let osd_target = osd::Target::from_request(method, &params);
        let before = osd_target
            .as_ref()
            .filter(|target| matches!(target, osd::Target::Mic(_)))
            .and_then(|target| target.read(&app));
        let result = registry::dispatch(&app, method, params);
        if let Some(target) = osd_target {
            let after = if result.is_ok() {
                target.read(&app)
            } else {
                None
            };
            for notice in target.notifications(before, after, result.is_ok()) {
                let _ = app.emit("local-api-osd", notice);
            }
        }
        // Even a failed command can have partially changed the graph. Re-read
        // backend state rather than making optimistic assumptions in the UI.
        if !read_only {
            let _ = app.emit("local-api-changed", method);
        }
        result
    });
    let state = ApiState {
        token: Arc::new(token),
        host: Arc::new(address.to_string()),
        gate: Arc::new(Semaphore::new(1)),
        dispatch,
    };
    std::thread::Builder::new()
        .name("mixweave-api".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            match runtime {
                Ok(runtime) => runtime.block_on(async move {
                    let listener =
                        tokio::net::TcpListener::from_std(listener).expect("bound API listener");
                    tokio::select! {
                        result = axum::serve(listener, router(state)) => {
                            if let Err(error) = result {
                                eprintln!("mixweave: API stopped: {error}");
                            }
                        }
                        _ = shutdown => {}
                    }
                }),
                Err(error) => eprintln!("mixweave: API runtime: {error}"),
            }
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::Request as HttpRequest,
    };
    use tower::ServiceExt;
    fn state() -> ApiState {
        ApiState {
            token: Arc::new("test-secret".into()),
            host: Arc::new("127.0.0.1:47832".into()),
            gate: Arc::new(Semaphore::new(1)),
            dispatch: Arc::new(|_, params| Ok(params)),
        }
    }
    fn request(path: &str) -> axum::http::request::Builder {
        HttpRequest::builder()
            .uri(path)
            .header("host", "127.0.0.1:47832")
            .header("authorization", "Bearer test-secret")
    }
    #[tokio::test]
    async fn authentication_and_browser_rejection() {
        let app = router(state());
        for (headers, expected) in [
            (vec![], 401),
            (vec![("authorization", "Bearer wrong")], 401),
            (
                vec![("authorization", "Bearer test-secret"), ("origin", "null")],
                403,
            ),
        ] {
            let mut r = HttpRequest::builder()
                .uri("/v1/health")
                .header("host", "127.0.0.1:47832");
            for (k, v) in headers {
                r = r.header(k, v);
            }
            assert_eq!(
                app.clone()
                    .oneshot(r.body(Body::empty()).unwrap())
                    .await
                    .unwrap()
                    .status()
                    .as_u16(),
                expected
            );
        }
        let r = HttpRequest::builder()
            .uri("/v1/health")
            .header("host", "evil.example")
            .header("authorization", "Bearer test-secret");
        assert_eq!(
            app.oneshot(r.body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    #[tokio::test]
    async fn catalog_and_rpc_contract() {
        let app = router(state());
        let response = app
            .clone()
            .oneshot(request("/v1/commands").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let catalog: Value = serde_json::from_slice(&body).unwrap();
        assert!(catalog["result"].as_array().unwrap().len() > 80);
        for (body, expected) in [
            (
                json!({"method":"set_channel_volume","params":{"sink_name":"sink_game","volume":50}}),
                200,
            ),
            (json!({"method":"reset_app"}), 404),
            (
                json!({"method":"set_channel_volume","params":{"volume":50}}),
                400,
            ),
            (
                json!({"method":"get_mic_config","params":{"unexpected":1}}),
                400,
            ),
            (json!({"method":"get_mic_config","params":[]}), 400),
        ] {
            let r = request("/v1/rpc")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap();
            assert_eq!(
                app.clone().oneshot(r).await.unwrap().status().as_u16(),
                expected
            );
        }
    }
    #[tokio::test]
    async fn busy_and_body_limits() {
        let s = state();
        let _permit = s.gate.clone().acquire_owned().await.unwrap();
        let app = router(s);
        let r = request("/v1/rpc")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"method":"get_mic_config"}"#))
            .unwrap();
        assert_eq!(
            app.clone().oneshot(r).await.unwrap().status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        let r = request("/v1/rpc")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(vec![b' '; 1024 * 1024 + 1]))
            .unwrap();
        assert_eq!(
            app.oneshot(r).await.unwrap().status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }
    #[test]
    fn typed_arguments_reject_invalid_values() {
        assert!(arg::<u8>(&json!({"volume": -1}), "volume").is_err());
        assert!(arg::<u8>(&json!({"volume": 256}), "volume").is_err());
        assert!(arg::<bool>(&json!({"muted": "true"}), "muted").is_err());
        assert_eq!(
            arg::<Option<String>>(&json!({}), "expected_profile").unwrap(),
            None
        );
    }
}
