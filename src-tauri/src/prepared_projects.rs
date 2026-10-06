//! Machine-local prepared-project state for DF AG Agency.
//!
//! Canonical project identity/context remains outside this app (DigitalFlow /
//! Command Center). This cache stores only the latest preparation result needed
//! to make a successfully prepared project visible in the Projects UI even when
//! zero agents are installed.

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    agent_runtime::{
        BootstrapAction, CorpusEvidence, DiscoveryRecord, PreparationResult, PreparedCapability,
        ProjectContext, Readiness,
    },
    error::AppError,
    state::AppState,
    util::fs::{atomic_write, read_capped},
};

const SCHEMA_VERSION: &str = "1.0.0";
const MAX_PREPARED_PROJECTS_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreparedProjectRecord {
    pub schema_version: String,
    pub project_id: String,
    pub display_name: String,
    pub client_or_owner: String,
    pub project_type: String,
    pub readiness: Readiness,
    pub context_status: String,
    pub agents: Vec<PreparedCapability>,
    pub required_capabilities: Vec<PreparedCapability>,
    pub recommended_capabilities: Vec<PreparedCapability>,
    pub excluded_capabilities: Vec<PreparedCapability>,
    pub gaps: Vec<PreparedCapability>,
    pub stale_states: Vec<DiscoveryRecord>,
    pub approval_actions: Vec<BootstrapAction>,
    pub next_operation: String,
    pub corpus_evidence: CorpusEvidence,
    pub prepared_at: String,
}

fn state_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("state")
}

fn prepared_projects_path(app_data_dir: &Path) -> PathBuf {
    state_dir(app_data_dir).join("prepared-projects.json")
}

async fn load(app_data_dir: &Path) -> Result<Vec<PreparedProjectRecord>, AppError> {
    let path = prepared_projects_path(app_data_dir);
    match tokio::fs::try_exists(&path).await {
        Ok(false) => return Ok(Vec::new()),
        Ok(true) => {}
        Err(e) => {
            return Err(AppError::Io {
                message: format!("stat {}: {e}", path.display()),
            })
        }
    }

    let bytes = read_capped(&path, MAX_PREPARED_PROJECTS_BYTES).await?;
    serde_json::from_slice(&bytes).map_err(|e| AppError::Io {
        message: format!("parse prepared-projects.json: {e}"),
    })
}

async fn save(app_data_dir: &Path, records: &[PreparedProjectRecord]) -> Result<(), AppError> {
    let dir = state_dir(app_data_dir);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| AppError::Io {
            message: format!("create state dir {}: {e}", dir.display()),
        })?;

    let bytes = serde_json::to_vec_pretty(records).map_err(|e| AppError::Io {
        message: format!("serialize prepared-projects.json: {e}"),
    })?;
    if bytes.len() as u64 > MAX_PREPARED_PROJECTS_BYTES {
        return Err(AppError::Io {
            message: format!(
                "prepared-projects.json exceeds {} bytes",
                MAX_PREPARED_PROJECTS_BYTES
            ),
        });
    }
    atomic_write(&prepared_projects_path(app_data_dir), &bytes).await
}

fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !out.is_empty() && !dash {
            out.push('-');
            dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn title_case_slug(value: &str) -> String {
    value
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!(
                    "{}{}",
                    first.to_ascii_uppercase(),
                    chars.as_str().to_ascii_lowercase()
                ),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn display_name(context: &ProjectContext) -> String {
    let owner = context
        .project_context
        .client_or_owner
        .split('/')
        .next()
        .unwrap_or(context.project_context.client_or_owner.as_str())
        .trim();

    let owner_slug = slugify(owner);
    let first_owner = owner_slug.split('-').next().unwrap_or("");
    let mut remainder = context.project_id.as_str();

    if !owner_slug.is_empty() {
        let full_prefix = format!("{owner_slug}-");
        if remainder.starts_with(&full_prefix) {
            remainder = &remainder[full_prefix.len()..];
        } else if !first_owner.is_empty() {
            let short_prefix = format!("{first_owner}-");
            if remainder.starts_with(&short_prefix) {
                remainder = &remainder[short_prefix.len()..];
            }
        }
    }

    let title = title_case_slug(remainder);
    if owner.is_empty() {
        title
    } else if title.is_empty() {
        owner.to_string()
    } else {
        format!("{owner} — {title}")
    }
}

pub async fn upsert(
    app_data_dir: &Path,
    context: &ProjectContext,
    result: &PreparationResult,
) -> Result<PreparedProjectRecord, AppError> {
    let record = PreparedProjectRecord {
        schema_version: SCHEMA_VERSION.into(),
        project_id: result.project_id.clone(),
        display_name: display_name(context),
        client_or_owner: context.project_context.client_or_owner.clone(),
        project_type: context.project_context.project_type.clone(),
        readiness: result.readiness.clone(),
        context_status: result.context_status.clone(),
        agents: result.agents.clone(),
        required_capabilities: result.required_capabilities.clone(),
        recommended_capabilities: result.recommended_capabilities.clone(),
        excluded_capabilities: result.excluded_capabilities.clone(),
        gaps: result.gaps.clone(),
        stale_states: result.stale_states.clone(),
        approval_actions: result.approval_actions.clone(),
        next_operation: result.next_operation.clone(),
        corpus_evidence: result.corpus_evidence.clone(),
        prepared_at: Utc::now().to_rfc3339(),
    };

    let mut records = load(app_data_dir).await?;
    if let Some(existing) = records
        .iter_mut()
        .find(|item| item.project_id == record.project_id)
    {
        *existing = record.clone();
    } else {
        records.push(record.clone());
    }
    records.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    save(app_data_dir, &records).await?;
    Ok(record)
}

#[tauri::command]
pub async fn prepared_projects_list(
    state: State<'_, AppState>,
) -> Result<Vec<PreparedProjectRecord>, AppError> {
    load(&state.app_data_dir).await
}

#[tauri::command]
pub async fn prepared_project_get(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Option<PreparedProjectRecord>, AppError> {
    Ok(load(&state.app_data_dir)
        .await?
        .into_iter()
        .find(|item| item.project_id == project_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::{CapabilityStatus, ProjectContextFields};

    fn cap(id: &str) -> PreparedCapability {
        PreparedCapability {
            capability_id: id.into(),
            capability_type: "ACTIVE_AGENT".into(),
            reason: "test".into(),
            status: CapabilityStatus::MatchedExisting,
            evidence_ref: None,
        }
    }

    fn context() -> ProjectContext {
        ProjectContext {
            project_id: "vrc-video-offer-landing".into(),
            project_context: ProjectContextFields {
                client_or_owner: "VRC Agency / owner review".into(),
                project_type: "partner landing page / static package configurator".into(),
                objective_or_problem: "test objective".into(),
                deliverables: vec!["landing".into()],
                scope: vec!["prototype".into()],
                existing_stack: vec!["Static HTML".into()],
                connected_services: vec![],
                constraints: vec![],
                approval_owner: "Diogo".into(),
                existing_capabilities: vec!["git".into()],
            },
        }
    }

    fn result() -> PreparationResult {
        PreparationResult {
            project_id: "vrc-video-offer-landing".into(),
            readiness: Readiness::ReadyWithWarnings,
            context_status: "SUFFICIENT".into(),
            agents: vec![cap("design-system-foundation-agent")],
            required_capabilities: vec![],
            recommended_capabilities: vec![],
            excluded_capabilities: vec![],
            gaps: vec![],
            stale_states: vec![],
            approval_actions: vec![],
            bootstrap_actions: vec![],
            evidence: vec![],
            provenance: vec![],
            missing_context: vec![],
            next_operation: "REVIEW_WARNINGS".into(),
            corpus_evidence: CorpusEvidence {
                manifest_schema_version: "1.0.0".into(),
                generation_id: "g1".into(),
                version: "v1".into(),
                provenance: "bundled".into(),
                fetched_at: "2026-10-06T00:00:00Z".into(),
                generated_at: "2026-10-06T00:00:00Z".into(),
                freshness: "FRESH".into(),
                integrity: "VALID".into(),
                count: 26,
            },
        }
    }

    #[tokio::test]
    async fn upsert_persists_prepared_project_without_install_or_path_identity() {
        let dir = tempfile::tempdir().unwrap();
        let stored = upsert(dir.path(), &context(), &result()).await.unwrap();
        assert_eq!(stored.project_id, "vrc-video-offer-landing");
        assert_eq!(stored.display_name, "VRC Agency — Video Offer Landing");
        assert_eq!(stored.agents.len(), 1);

        let loaded = load(dir.path()).await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].readiness, Readiness::ReadyWithWarnings);
    }

    #[tokio::test]
    async fn upsert_replaces_latest_result_by_project_id() {
        let dir = tempfile::tempdir().unwrap();
        upsert(dir.path(), &context(), &result()).await.unwrap();

        let mut next = result();
        next.readiness = Readiness::Ready;
        next.next_operation = "CONTINUE_WORK".into();
        upsert(dir.path(), &context(), &next).await.unwrap();

        let loaded = load(dir.path()).await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].readiness, Readiness::Ready);
        assert_eq!(loaded[0].next_operation, "CONTINUE_WORK");
    }
}
