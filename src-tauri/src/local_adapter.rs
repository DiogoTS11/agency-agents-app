//! App-owned localhost transport for the preparation runtime.
//!
//! This module is deliberately thin: it authenticates and validates the wire
//! request, obtains the already-validated App corpus snapshot, calls the
//! semantic runtime, and maps the result to the ratified HTTP envelope. It
//! does not resolve project names, read DigitalFlow registries, or implement
//! Discovery/Bootstrap semantics a second time.

use std::{path::{Path, PathBuf}, sync::Arc};

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    net::TcpListener,
    sync::{oneshot, Mutex},
    task::JoinHandle,
};
use uuid::Uuid;

use crate::{
    agent_runtime::{
        self, BootstrapAction, CorpusEvidence, DiscoveryRecord, EnvironmentEvidence,
        PreparationResult, PreparedCapability, ProjectContext, ProjectContextFields, RuntimeError,
        PREPARE_PROJECT_CONTRACT_VERSION,
    },
    corpus,
    error::AppError,
    github::auth::{KeychainSlot, SystemKeychain},
    prepared_projects,
};

pub const LOOPBACK_HOST: &str = "127.0.0.1";
pub const AUTH_REF: &str = "agent-agency-runtime-v1";
pub const CONTRACT_VERSION: &str = PREPARE_PROJECT_CONTRACT_VERSION;
const MAX_REQUEST_BYTES: usize = 1_048_576;
const DEFAULT_CORPUS_MAX_AGE_SECONDS: i64 = 86_400;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RuntimeDescriptor {
    pub host: String,
    pub port: u16,
    pub contract_version: String,
    pub auth_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalAdapterConfig {
    pub port: u16,
    pub corpus_max_age_seconds: i64,
}

#[derive(Debug, Deserialize)]
struct MachineRuntimeDescriptor {
    host: String,
    port: u16,
    contract_version: String,
    auth_ref: String,
}

#[derive(Debug, Deserialize)]
struct MachineRuntimeConfig {
    schema_version: String,
    corpus_max_age_seconds: i64,
    agent_agency_runtime: Option<MachineRuntimeDescriptor>,
}

/// Read the already-ratified machine-local descriptor. The App reads only this
/// machine config; it never reads Command Center registries or project paths.
pub fn load_machine_runtime_config() -> Result<LocalAdapterConfig, AdapterError> {
    let home = dirs::home_dir().ok_or(AdapterError::DescriptorUnavailable)?;
    load_machine_runtime_config_from(&home.join(".digital-flow-os").join("os-paths.json"))
}

fn load_machine_runtime_config_from(path: &Path) -> Result<LocalAdapterConfig, AdapterError> {
    let raw = std::fs::read_to_string(path).map_err(|_| AdapterError::DescriptorUnavailable)?;
    let config: MachineRuntimeConfig = serde_json::from_str(&raw)
        .map_err(|_| AdapterError::DescriptorInvalid)?;
    if config.schema_version != "1.1.0" {
        return Err(AdapterError::DescriptorInvalid);
    }
    let descriptor = config.agent_agency_runtime.ok_or(AdapterError::DescriptorMissing)?;
    if descriptor.host != LOOPBACK_HOST
        || descriptor.contract_version != CONTRACT_VERSION
        || descriptor.auth_ref != AUTH_REF
        || config.corpus_max_age_seconds < 1
    {
        return Err(AdapterError::DescriptorInvalid);
    }
    let mut adapter = LocalAdapterConfig::new(descriptor.port)?;
    adapter.corpus_max_age_seconds = config.corpus_max_age_seconds;
    Ok(adapter)
}

impl LocalAdapterConfig {
    pub fn new(port: u16) -> Result<Self, AdapterError> {
        if port == 0 {
            return Err(AdapterError::InvalidPort);
        }
        Ok(Self {
            port,
            corpus_max_age_seconds: DEFAULT_CORPUS_MAX_AGE_SECONDS,
        })
    }
}

pub trait CredentialStore: Send + Sync {
    fn read(&self, auth_ref: &str) -> Result<Option<String>, AppError>;
    fn write(&self, auth_ref: &str, token: &str) -> Result<(), AppError>;
}

pub struct SystemCredentialStore;

impl CredentialStore for SystemCredentialStore {
    fn read(&self, auth_ref: &str) -> Result<Option<String>, AppError> {
        KeychainSlot::read(&SystemKeychain, auth_ref)
    }

    fn write(&self, auth_ref: &str, token: &str) -> Result<(), AppError> {
        KeychainSlot::write(&SystemKeychain, auth_ref, token)
    }
}

pub fn provision_token(store: &dyn CredentialStore) -> Result<String, AdapterError> {
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    store
        .write(AUTH_REF, &token)
        .map_err(AdapterError::CredentialStore)?;
    Ok(token)
}

pub fn rotate_token(store: &dyn CredentialStore) -> Result<String, AdapterError> {
    provision_token(store)
}

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("agent agency runtime descriptor is unavailable")]
    DescriptorUnavailable,
    #[error("agent agency runtime descriptor is missing")]
    DescriptorMissing,
    #[error("agent agency runtime descriptor is invalid")]
    DescriptorInvalid,
    #[error("runtime adapter port must be an explicitly configured non-zero value")]
    InvalidPort,
    #[error("runtime adapter is already running")]
    AlreadyRunning,
    #[error("runtime adapter could not bind to loopback port")]
    PortConflict,
    #[error("runtime adapter credential store unavailable")]
    CredentialStore(AppError),
    #[error("runtime adapter credential is missing")]
    CredentialUnavailable,
}

struct ActiveAdapter {
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

#[derive(Clone)]
pub struct LocalAdapterManager {
    active: Arc<Mutex<Option<ActiveAdapter>>>,
}

impl Default for LocalAdapterManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalAdapterManager {
    pub fn new() -> Self {
        Self {
            active: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn start(
        &self,
        config: LocalAdapterConfig,
        app_data_dir: PathBuf,
        credentials: Arc<dyn CredentialStore>,
    ) -> Result<RuntimeDescriptor, AdapterError> {
        let mut active = self.active.lock().await;
        if active.is_some() {
            return Err(AdapterError::AlreadyRunning);
        }
        match credentials
            .read(AUTH_REF)
            .map_err(AdapterError::CredentialStore)?
        {
            Some(token) if !token.is_empty() => {}
            _ => return Err(AdapterError::CredentialUnavailable),
        }

        let listener = TcpListener::bind((LOOPBACK_HOST, config.port))
            .await
            .map_err(|_| AdapterError::PortConflict)?;
        let port = listener
            .local_addr()
            .map_err(|_| AdapterError::PortConflict)?
            .port();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let state = AdapterState {
            app_data_dir,
            corpus_max_age_seconds: config.corpus_max_age_seconds,
            credentials,
        };
        let task = tokio::spawn(async move {
            serve(listener, state, shutdown_rx).await;
        });
        *active = Some(ActiveAdapter {
            shutdown: shutdown_tx,
            task,
        });

        Ok(RuntimeDescriptor {
            host: LOOPBACK_HOST.into(),
            port,
            contract_version: CONTRACT_VERSION.into(),
            auth_ref: AUTH_REF.into(),
        })
    }

    pub async fn stop(&self) {
        let active = self.active.lock().await.take();
        if let Some(active) = active {
            let _ = active.shutdown.send(());
            let _ = active.task.await;
        }
    }

    pub async fn is_running(&self) -> bool {
        self.active.lock().await.is_some()
    }
}

#[derive(Clone)]
struct AdapterState {
    app_data_dir: PathBuf,
    corpus_max_age_seconds: i64,
    credentials: Arc<dyn CredentialStore>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrepareProjectRequest {
    contract_version: String,
    project_id: String,
    project_context: ProjectContextFields,
    environment_evidence: EnvironmentEvidence,
}

#[derive(Debug, Serialize)]
struct SuccessEnvelope {
    ok: bool,
    contract_version: &'static str,
    #[serde(flatten)]
    result: WirePreparationResult,
}

#[derive(Debug, Serialize)]
struct WirePreparationResult {
    project_id: String,
    readiness: agent_runtime::Readiness,
    context_status: String,
    agents: Vec<PreparedCapability>,
    required_capabilities: Vec<PreparedCapability>,
    recommended_capabilities: Vec<PreparedCapability>,
    excluded_capabilities: Vec<PreparedCapability>,
    gaps: Vec<PreparedCapability>,
    stale_states: Vec<DiscoveryRecord>,
    approval_actions: Vec<BootstrapAction>,
    next_operation: String,
    corpus_evidence: CorpusEvidence,
}

impl From<PreparationResult> for WirePreparationResult {
    fn from(result: PreparationResult) -> Self {
        Self {
            project_id: result.project_id,
            readiness: result.readiness,
            context_status: result.context_status,
            agents: result.agents,
            required_capabilities: result.required_capabilities,
            recommended_capabilities: result.recommended_capabilities,
            excluded_capabilities: result.excluded_capabilities,
            gaps: result.gaps,
            stale_states: result.stale_states,
            approval_actions: result.approval_actions,
            next_operation: result.next_operation,
            corpus_evidence: result.corpus_evidence,
        }
    }
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    ok: bool,
    contract_version: &'static str,
    error: WireError,
}

#[derive(Debug, Serialize)]
struct WireError {
    code: &'static str,
    message: &'static str,
    retryable: bool,
    class: &'static str,
}

async fn serve(listener: TcpListener, state: AdapterState, shutdown: oneshot::Receiver<()>) {
    let app = Router::new()
        .route("/api/agent/prepare-project", post(prepare_project))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .with_state(state);
    let _ = axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = shutdown.await;
        })
        .await;
}

async fn prepare_project(
    State(state): State<AdapterState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !authenticate(&headers, state.credentials.as_ref()) {
        return error_response(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED_LOCAL_CALLER",
            "unauthorized local caller",
            false,
            "INFRASTRUCTURE",
        );
    }
    if body.len() > MAX_REQUEST_BYTES {
        return error_response(
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
            "request is too large",
            false,
            "INFRASTRUCTURE",
        );
    }
    let request: PrepareProjectRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "INVALID_REQUEST",
                "request is invalid",
                false,
                "INFRASTRUCTURE",
            )
        }
    };
    if request.contract_version != CONTRACT_VERSION {
        return error_response(
            StatusCode::CONFLICT,
            "CONTRACT_VERSION_MISMATCH",
            "unsupported contract version",
            false,
            "INFRASTRUCTURE",
        );
    }

    let context = ProjectContext {
        project_id: request.project_id,
        project_context: request.project_context,
    };
    let snapshot = match corpus::read_validated_snapshot(
        &state.app_data_dir,
        state.corpus_max_age_seconds,
        Utc::now(),
    )
    .await
    {
        Ok(snapshot) => snapshot,
        Err(error) => return map_app_error(error),
    };
    let result =
        match agent_runtime::prepare_project(&context, &request.environment_evidence, &snapshot) {
            Ok(result) => result,
            Err(error) => return map_runtime_error(error),
        };
    if let Err(error) = prepared_projects::upsert(&state.app_data_dir, &context, &result).await {
        return map_app_error(error);
    }
    (
        StatusCode::OK,
        Json(SuccessEnvelope {
            ok: true,
            contract_version: CONTRACT_VERSION,
            result: result.into(),
        }),
    )
        .into_response()
}

fn authenticate(headers: &HeaderMap, credentials: &dyn CredentialStore) -> bool {
    let supplied = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty());
    let expected = credentials.read(AUTH_REF).ok().flatten();
    match (supplied, expected.as_deref()) {
        (Some(supplied), Some(expected)) => {
            constant_time_equal(supplied.as_bytes(), expected.as_bytes())
        }
        _ => false,
    }
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let length_difference = left.len() ^ right.len();
    let max = left.len().max(right.len());
    let mut difference = length_difference as u8;
    for index in 0..max {
        difference |=
            left.get(index).copied().unwrap_or(0) ^ right.get(index).copied().unwrap_or(0);
    }
    difference == 0
}

fn error_response(
    status: StatusCode,
    code: &'static str,
    message: &'static str,
    retryable: bool,
    class: &'static str,
) -> Response {
    (
        status,
        Json(ErrorEnvelope {
            ok: false,
            contract_version: CONTRACT_VERSION,
            error: WireError {
                code,
                message,
                retryable,
                class,
            },
        }),
    )
        .into_response()
}

fn map_app_error(error: AppError) -> Response {
    let (status, code, retryable) = match error {
        AppError::CorpusManifestMissing
        | AppError::CorpusIndexMissing
        | AppError::CorpusMetaMissing
        | AppError::CorpusStateMissing => {
            (StatusCode::SERVICE_UNAVAILABLE, "CORPUS_NOT_FOUND", false)
        }
        AppError::CorpusIndexStale => (StatusCode::SERVICE_UNAVAILABLE, "CORPUS_STALE", false),
        AppError::CorpusSnapshotInconsistent
        | AppError::CorpusIndexMalformed
        | AppError::CorpusMetaMalformed
        | AppError::CorpusManifestMalformed
        | AppError::CorpusSchemaUnsupported
        | AppError::CorpusCountMismatch => (
            StatusCode::SERVICE_UNAVAILABLE,
            "CORPUS_INTEGRITY_FAILED",
            false,
        ),
        AppError::CorpusRosterMismatch { .. } => {
            (StatusCode::SERVICE_UNAVAILABLE, "PREPARATION_FAILED", false)
        }
        AppError::KeychainUnavailable { .. } => {
            (StatusCode::SERVICE_UNAVAILABLE, "RUNTIME_NOT_READY", true)
        }
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_RUNTIME_ERROR",
            false,
        ),
    };
    error_response(
        status,
        code,
        "runtime request could not be completed",
        retryable,
        "INFRASTRUCTURE",
    )
}

fn map_runtime_error(error: RuntimeError) -> Response {
    let (status, code, class) = match error {
        RuntimeError::InvalidProjectContext(_) | RuntimeError::InvalidEnvironmentEvidence(_) => {
            (StatusCode::BAD_REQUEST, "INVALID_REQUEST", "INFRASTRUCTURE")
        }
        RuntimeError::RosterIncomplete(_) | RuntimeError::PreparationFailed(_) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "PREPARATION_FAILED",
            "BUSINESS",
        ),
        RuntimeError::ContextInsufficient(_) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "CONTEXT_INSUFFICIENT",
            "BUSINESS",
        ),
    };
    error_response(status, code, "request cannot be prepared", false, class)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;
    use tokio::net::TcpListener;

    struct MockCredentials(StdMutex<HashMap<String, String>>);

    impl MockCredentials {
        fn new() -> Self {
            Self(StdMutex::new(HashMap::new()))
        }
    }

    impl CredentialStore for MockCredentials {
        fn read(&self, auth_ref: &str) -> Result<Option<String>, AppError> {
            Ok(self.0.lock().unwrap().get(auth_ref).cloned())
        }

        fn write(&self, auth_ref: &str, token: &str) -> Result<(), AppError> {
            self.0.lock().unwrap().insert(auth_ref.into(), token.into());
            Ok(())
        }
    }

    #[test]
    fn only_loopback_is_a_valid_binding_target() {
        assert_eq!(LOOPBACK_HOST, "127.0.0.1");
        assert!(LocalAdapterConfig::new(4315).is_ok());
        assert!(LocalAdapterConfig::new(0).is_err());
    }

    fn write_descriptor(value: serde_json::Value) -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("os-paths.json"), serde_json::to_vec(&value).unwrap()).unwrap();
        temp
    }

    fn valid_descriptor() -> serde_json::Value {
        json!({
            "schema_version": "1.1.0",
            "corpus_max_age_seconds": 86400,
            "agent_agency_runtime": {
                "host": "127.0.0.1", "port": 4315,
                "contract_version": "1.1.0", "auth_ref": "agent-agency-runtime-v1"
            }
        })
    }

    #[test]
    fn machine_descriptor_loads_configured_loopback_port_without_a_default() {
        let temp = write_descriptor(valid_descriptor());
        let config = load_machine_runtime_config_from(&temp.path().join("os-paths.json")).unwrap();
        assert_eq!(config.port, 4315);
        assert_eq!(config.corpus_max_age_seconds, 86400);
    }

    #[test]
    fn machine_descriptor_rejects_missing_invalid_or_non_loopback_values() {
        let missing = write_descriptor(json!({"schema_version":"1.1.0", "corpus_max_age_seconds":86400}));
        assert!(matches!(
            load_machine_runtime_config_from(&missing.path().join("os-paths.json")),
            Err(AdapterError::DescriptorMissing)
        ));

        let mut invalid_host = valid_descriptor();
        invalid_host["agent_agency_runtime"]["host"] = json!("0.0.0.0");
        let invalid_host = write_descriptor(invalid_host);
        assert!(matches!(
            load_machine_runtime_config_from(&invalid_host.path().join("os-paths.json")),
            Err(AdapterError::DescriptorInvalid)
        ));

        let invalid_schema = write_descriptor(json!({
            "schema_version":"1.0.0", "corpus_max_age_seconds":86400,
            "agent_agency_runtime": valid_descriptor()["agent_agency_runtime"].clone()
        }));
        assert!(matches!(
            load_machine_runtime_config_from(&invalid_schema.path().join("os-paths.json")),
            Err(AdapterError::DescriptorInvalid)
        ));
    }

    #[test]
    fn token_is_random_and_rotation_replaces_the_old_value() {
        let credentials = MockCredentials::new();
        let first = provision_token(&credentials).unwrap();
        let second = rotate_token(&credentials).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            credentials.read(AUTH_REF).unwrap().as_deref(),
            Some(second.as_str())
        );
        assert!(!first.contains(' '));
    }

    #[test]
    fn bearer_auth_is_fail_closed_and_constant_time_comparison_is_exact() {
        assert!(constant_time_equal(b"secret", b"secret"));
        assert!(!constant_time_equal(b"secret", b"secreT"));
        assert!(!constant_time_equal(b"secret", b"secret-longer"));
    }

    #[test]
    fn authorization_requires_exact_bearer_token() {
        let credentials = MockCredentials::new();
        credentials.write(AUTH_REF, "expected-token").unwrap();
        let mut headers = HeaderMap::new();
        assert!(!authenticate(&headers, &credentials));
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic expected-token"),
        );
        assert!(!authenticate(&headers, &credentials));
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer wrong-token"),
        );
        assert!(!authenticate(&headers, &credentials));
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer expected-token"),
        );
        assert!(authenticate(&headers, &credentials));
    }

    #[test]
    fn missing_credentials_fail_closed_before_binding() {
        let credentials = MockCredentials::new();
        let result = LocalAdapterConfig::new(4315).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let error = runtime.block_on(async {
            LocalAdapterManager::new()
                .start(
                    result,
                    tempfile::tempdir().unwrap().path().to_path_buf(),
                    Arc::new(credentials),
                )
                .await
                .unwrap_err()
        });
        assert!(matches!(error, AdapterError::CredentialUnavailable));
    }

    #[tokio::test]
    async fn wire_validation_rejects_unknown_fields_and_semantic_evidence() {
        let credentials = Arc::new(MockCredentials::new());
        let token = provision_token(credentials.as_ref()).unwrap();
        let state = AdapterState {
            app_data_dir: tempfile::tempdir().unwrap().path().to_path_buf(),
            corpus_max_age_seconds: DEFAULT_CORPUS_MAX_AGE_SECONDS,
            credentials,
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );

        let mut request = json!({
            "contract_version": CONTRACT_VERSION,
            "project_id": "atom-website",
            "project_context": {
                "client_or_owner": "Thomas", "project_type": "client-project", "objective_or_problem": "Launch site",
                "deliverables": ["site"], "scope": ["phase 1"], "existing_stack": ["Next.js custom"],
                "connected_services": ["Vercel"], "constraints": ["legal"], "approval_owner": "diogo",
                "existing_capabilities": ["git"]
            },
            "environment_evidence": {
                "evidence_schema_version": "1.0.0", "generated_at": "2026-09-21T10:00:00Z",
                "source_categories_consulted": ["cli_probe"], "observations": []
            }
        });
        request["repoRoot"] = json!("C:/forbidden");
        let response = prepare_project(
            State(state.clone()),
            headers.clone(),
            Bytes::from(serde_json::to_vec(&request).unwrap()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        request.as_object_mut().unwrap().remove("repoRoot");
        request["environment_evidence"]["observations"] = json!([{
            "capability_id": "git-cli", "observations": [{
                "source_category": "cli_probe", "kind": "cli_probe", "result": "STALE", "observed_at": "2026-09-21T10:00:00Z"
            }]
        }]);
        let response = prepare_project(
            State(state),
            headers,
            Bytes::from(serde_json::to_vec(&request).unwrap()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn authenticated_request_maps_missing_corpus_without_fallback() {
        let credentials = Arc::new(MockCredentials::new());
        let token = provision_token(credentials.as_ref()).unwrap();
        let state = AdapterState {
            app_data_dir: tempfile::tempdir().unwrap().path().to_path_buf(),
            corpus_max_age_seconds: DEFAULT_CORPUS_MAX_AGE_SECONDS,
            credentials,
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );
        let request = json!({
            "contract_version": CONTRACT_VERSION,
            "project_id": "agent-agency",
            "project_context": {
                "client_or_owner": "DF", "project_type": "internal", "objective_or_problem": "Runtime",
                "deliverables": ["runtime"], "scope": ["adapter"], "existing_stack": ["Rust"],
                "connected_services": ["none"], "constraints": ["local"], "approval_owner": "diogo",
                "existing_capabilities": ["git"]
            },
            "environment_evidence": {
                "evidence_schema_version": "1.0.0", "generated_at": "2026-09-21T10:00:00Z",
                "source_categories_consulted": ["cli_probe"], "observations": []
            }
        });
        let response = prepare_project(
            State(state),
            headers,
            Bytes::from(serde_json::to_vec(&request).unwrap()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn real_loopback_http_rejects_unauthorized_request() {
        let manager = LocalAdapterManager::new();
        let credentials = Arc::new(MockCredentials::new());
        provision_token(credentials.as_ref()).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let descriptor = manager
            .start(
                LocalAdapterConfig::new(find_free_port().await).unwrap(),
                temp.path().to_path_buf(),
                credentials,
            )
            .await
            .unwrap();
        let response = reqwest::Client::new()
            .post(format!(
                "http://{}:{}/api/agent/prepare-project",
                descriptor.host, descriptor.port
            ))
            .body("{}")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
        manager.stop().await;
    }

    #[test]
    fn corpus_failures_map_to_distinct_503_codes() {
        assert_eq!(
            map_app_error(AppError::CorpusManifestMissing).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            map_app_error(AppError::CorpusIndexStale).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            map_app_error(AppError::CorpusSnapshotInconsistent).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            map_runtime_error(RuntimeError::RosterIncomplete("missing".into())).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[tokio::test]
    async fn duplicate_start_is_rejected_and_stop_is_clean() {
        let manager = LocalAdapterManager::new();
        let credentials = Arc::new(MockCredentials::new());
        provision_token(credentials.as_ref()).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let config = LocalAdapterConfig::new(find_free_port().await).unwrap();
        manager
            .start(config, temp.path().to_path_buf(), credentials.clone())
            .await
            .unwrap();
        let duplicate = manager
            .start(config, temp.path().to_path_buf(), credentials)
            .await;
        assert!(matches!(duplicate, Err(AdapterError::AlreadyRunning)));
        assert!(manager.is_running().await);
        manager.stop().await;
        assert!(!manager.is_running().await);
    }

    #[tokio::test]
    async fn port_conflict_is_visible_and_does_not_rebind() {
        let occupied = TcpListener::bind((LOOPBACK_HOST, 0)).await.unwrap();
        let port = occupied.local_addr().unwrap().port();
        let manager = LocalAdapterManager::new();
        let credentials = Arc::new(MockCredentials::new());
        provision_token(credentials.as_ref()).unwrap();
        let result = manager
            .start(
                LocalAdapterConfig::new(port).unwrap(),
                tempfile::tempdir().unwrap().path().to_path_buf(),
                credentials,
            )
            .await;
        assert!(matches!(result, Err(AdapterError::PortConflict)));
        drop(occupied);
        assert!(!manager.is_running().await);
    }

    async fn find_free_port() -> u16 {
        let listener = TcpListener::bind((LOOPBACK_HOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    }
}
