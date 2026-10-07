//! App-owned semantic runtime for project preparation.
//!
//! This module is deliberately transport- and filesystem-independent. DigitalFlow
//! supplies normalized project context and factual environment evidence; the App
//! supplies a snapshot that has already passed the manifest-backed corpus reader.
//! No registry, repo root, path, subprocess, socket, or HTTP concept belongs here.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::capability_registry::{self, MissingBehavior};
use crate::corpus::CorpusSnapshot;

pub const PROJECT_CONTEXT_SCHEMA_VERSION: &str = "1.0.0";
pub const ENVIRONMENT_EVIDENCE_SCHEMA_VERSION: &str = "1.0.0";
pub const PREPARE_PROJECT_CONTRACT_VERSION: &str = "1.1.0";

const RECONCILED_ROSTER_SLUGS: [&str; 26] = [
    "design-system-foundation-agent", "visual-prompt-engineer",
    "micro-interaction-delight-specialist", "design-ui-finish-gate-reviewer",
    "marketing-strategy-brief-agent", "marketing-research-intelligence-agent",
    "marketing-content-copy-agent", "marketing-lifecycle-crm-agent",
    "marketing-reputation-crisis-agent", "marketing-search-ai-visibility-agent",
    "marketing-review-growth-agent", "paid-media-auditor",
    "paid-media-ppc-strategist", "paid-media-paid-social-strategist",
    "paid-media-programmatic-buyer", "paid-media-creative-strategist",
    "paid-media-search-query-analyst", "paid-media-tracking-specialist",
    "research-synthesist", "product-manager",
    "project-management-jira-workflow-steward",
    "project-management-meeting-notes-specialist",
    "engineering-web-implementation-agent", "engineering-web-performance-engineer",
    "engineering-ecommerce-platform-engineer",
    "engineering-section-508-accessibility-reviewer",
];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RuntimeError {
    #[error("invalid project context: {0}")]
    InvalidProjectContext(String),
    #[error("invalid environment evidence: {0}")]
    InvalidEnvironmentEvidence(String),
    #[error("project context is insufficient: {0:?}")]
    ContextInsufficient(Vec<String>),
    #[error("validated App roster is incomplete: {0}")]
    RosterIncomplete(String),
    #[error("preparation failed: {0}")]
    PreparationFailed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectContext {
    pub project_id: String,
    pub project_context: ProjectContextFields,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectContextFields {
    pub client_or_owner: String,
    pub project_type: String,
    pub objective_or_problem: String,
    pub deliverables: Vec<String>,
    pub scope: Vec<String>,
    pub existing_stack: Vec<String>,
    pub connected_services: Vec<String>,
    pub constraints: Vec<String>,
    pub approval_owner: String,
    pub existing_capabilities: Vec<String>,
}

impl ProjectContext {
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.project_id.is_empty()
            || self.project_id.len() > 128
            || !self.project_id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || self.project_id.starts_with('-')
            || self.project_id.chars().any(|character| matches!(character, '/' | '\\' | ':'))
        {
            return Err(RuntimeError::InvalidProjectContext("project_id is not a canonical identifier".into()));
        }
        let fields = &self.project_context;
        for (name, value) in [
            ("client_or_owner", &fields.client_or_owner),
            ("project_type", &fields.project_type),
            ("objective_or_problem", &fields.objective_or_problem),
            ("approval_owner", &fields.approval_owner),
        ] {
            validate_text(name, value, 4_000)?;
        }
        for (name, values) in [
            ("deliverables", &fields.deliverables),
            ("scope", &fields.scope),
            ("existing_stack", &fields.existing_stack),
            ("connected_services", &fields.connected_services),
            ("constraints", &fields.constraints),
            ("existing_capabilities", &fields.existing_capabilities),
        ] {
            if values.len() > 128 {
                return Err(RuntimeError::InvalidProjectContext(format!("{name} exceeds 128 items")));
            }
            let mut unique = BTreeSet::new();
            for value in values {
                validate_text(name, value, 1_000)?;
                if !unique.insert(value.trim().to_string()) {
                    return Err(RuntimeError::InvalidProjectContext(format!("{name} contains a duplicate")));
                }
            }
        }
        Ok(())
    }

    pub fn missing_fields(&self) -> Vec<String> {
        let f = &self.project_context;
        let mut missing = Vec::new();
        if f.client_or_owner.trim().is_empty() { missing.push("client_or_owner".into()); }
        if f.project_type.trim().is_empty() { missing.push("project_type".into()); }
        if f.objective_or_problem.trim().is_empty() { missing.push("objective_or_problem".into()); }
        if f.deliverables.is_empty() { missing.push("deliverables".into()); }
        if f.scope.is_empty() { missing.push("scope".into()); }
        if f.existing_stack.is_empty() { missing.push("existing_stack".into()); }
        if f.connected_services.is_empty() { missing.push("connected_services".into()); }
        if f.constraints.is_empty() { missing.push("constraints".into()); }
        if f.approval_owner.trim().is_empty() { missing.push("approval_owner".into()); }
        if f.existing_capabilities.is_empty() { missing.push("existing_capabilities".into()); }
        missing
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSourceCategory {
    CapabilityRegistry,
    PluginRegistry,
    ModuleRegistry,
    CapabilityMap,
    CliProbe,
    GovernedGstackEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind { RegistryEntry, CliProbe, FilesystemPresence, GstackPresence }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceResult { Present, Absent, SourceUnavailable, ProbeFailed }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentObservation {
    pub source_category: EvidenceSourceCategory,
    pub kind: EvidenceKind,
    pub result: EvidenceResult,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    pub observed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CapabilityObservations {
    pub capability_id: String,
    pub observations: Vec<EnvironmentObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentEvidence {
    pub evidence_schema_version: String,
    pub generated_at: String,
    pub source_categories_consulted: Vec<EvidenceSourceCategory>,
    pub observations: Vec<CapabilityObservations>,
}

impl EnvironmentEvidence {
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.evidence_schema_version != ENVIRONMENT_EVIDENCE_SCHEMA_VERSION {
            return Err(RuntimeError::InvalidEnvironmentEvidence("unsupported schema version".into()));
        }
        validate_timestamp(&self.generated_at, "generated_at")?;
        if self.source_categories_consulted.is_empty() {
            return Err(RuntimeError::InvalidEnvironmentEvidence("no source categories consulted".into()));
        }
        let mut categories = BTreeSet::new();
        for category in &self.source_categories_consulted {
            if !categories.insert(category) {
                return Err(RuntimeError::InvalidEnvironmentEvidence("duplicate source category".into()));
            }
        }
        for group in &self.observations {
            validate_capability_id(&group.capability_id)?;
            if group.observations.is_empty() || group.observations.len() > 16 {
                return Err(RuntimeError::InvalidEnvironmentEvidence(format!("{} must have 1..16 observations", group.capability_id)));
            }
            for observation in &group.observations {
                validate_timestamp(&observation.observed_at, "observed_at")?;
                if let Some(version) = &observation.version { validate_text("version", version, 256)?; }
                if let Some(detail) = &observation.detail { validate_text("detail", detail, 512)?; }
            }
        }
        Ok(())
    }

}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DiscoveryState {
    Registered, Detected, Available, Active, Missing, Unclassified, Stale,
    NeedsCanonicalization,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryRecord {
    pub id: String,
    pub discovery_status: Option<DiscoveryState>,
    pub detected_version: Option<String>,
    pub evidence: Vec<EnvironmentObservation>,
    pub evidence_problem: Option<String>,
}

pub fn discover(evidence: &EnvironmentEvidence) -> Result<Vec<DiscoveryRecord>, RuntimeError> {
    evidence.validate()?;
    Ok(evidence.observations.iter().map(discover_one).collect())
}

fn discover_one(group: &CapabilityObservations) -> DiscoveryRecord {
    let registered = group.observations.iter().any(|o| is_registry(&o.source_category) && o.result == EvidenceResult::Present);
    let detected = group.observations.iter().any(|o| is_detection(&o.source_category) && o.result == EvidenceResult::Present);
    let mismatch = group.observations.iter().any(|o| is_registry(&o.source_category) && o.result == EvidenceResult::Absent) && detected;
    let has_failure = group.observations.iter().any(|o| matches!(o.result, EvidenceResult::SourceUnavailable | EvidenceResult::ProbeFailed));
    let status = if group.capability_id == "gstack" {
        Some(DiscoveryState::NeedsCanonicalization)
    } else if mismatch {
        Some(DiscoveryState::Stale)
    } else if registered && detected && group.capability_id == "openai-codex-plugin" {
        Some(DiscoveryState::Active)
    } else if registered && detected {
        Some(DiscoveryState::Available)
    } else if registered && !detected && !has_failure {
        Some(DiscoveryState::Missing)
    } else if !registered && detected {
        Some(DiscoveryState::Unclassified)
    } else if registered && !detected {
        None
    } else {
        None
    };
    DiscoveryRecord {
        id: group.capability_id.clone(),
        discovery_status: status,
        detected_version: group.observations.iter().find_map(|o| o.version.clone()),
        evidence: group.observations.clone(),
        evidence_problem: has_failure.then(|| "environment evidence collection was incomplete".into()),
    }
}

fn is_registry(category: &EvidenceSourceCategory) -> bool {
    matches!(category, EvidenceSourceCategory::CapabilityRegistry | EvidenceSourceCategory::PluginRegistry | EvidenceSourceCategory::ModuleRegistry | EvidenceSourceCategory::CapabilityMap)
}

fn is_detection(category: &EvidenceSourceCategory) -> bool {
    matches!(category, EvidenceSourceCategory::CliProbe | EvidenceSourceCategory::GovernedGstackEvidence)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Readiness { Ready, ReadyWithWarnings, Blocked, ContextInsufficient }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapabilityStatus { MatchedExisting, Gap, NoEvidence, Available, Active, Stale, Verify }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PreparedCapability {
    pub capability_id: String,
    pub capability_type: String,
    pub reason: String,
    pub status: CapabilityStatus,
    pub evidence_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BootstrapAction {
    pub capability_id: String,
    pub action: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RosterAgent { pub slug: String, pub name: String, pub description: String }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CorpusEvidence {
    pub manifest_schema_version: String,
    pub generation_id: String,
    pub version: String,
    pub provenance: String,
    pub fetched_at: String,
    pub generated_at: String,
    pub freshness: String,
    pub integrity: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PreparationResult {
    pub project_id: String,
    pub readiness: Readiness,
    pub context_status: String,
    pub agents: Vec<PreparedCapability>,
    pub required_capabilities: Vec<PreparedCapability>,
    pub recommended_capabilities: Vec<PreparedCapability>,
    pub excluded_capabilities: Vec<PreparedCapability>,
    pub gaps: Vec<PreparedCapability>,
    pub stale_states: Vec<DiscoveryRecord>,
    pub approval_actions: Vec<BootstrapAction>,
    pub bootstrap_actions: Vec<BootstrapAction>,
    pub evidence: Vec<EnvironmentObservation>,
    pub provenance: Vec<String>,
    pub missing_context: Vec<String>,
    pub next_operation: String,
    pub corpus_evidence: CorpusEvidence,
}

pub fn prepare_project(
    context: &ProjectContext,
    evidence: &EnvironmentEvidence,
    snapshot: &CorpusSnapshot,
) -> Result<PreparationResult, RuntimeError> {
    context.validate()?;
    evidence.validate()?;
    let corpus_evidence = CorpusEvidence {
        manifest_schema_version: "1.0.0".into(),
        generation_id: snapshot.generation_id.clone(),
        version: snapshot.meta.version.clone(),
        provenance: snapshot.provenance.into(),
        fetched_at: snapshot.meta.fetched_at.clone(),
        generated_at: snapshot.generated_at.clone(),
        freshness: "FRESH".into(),
        integrity: "VALID".into(),
        count: snapshot.meta.count,
    };
    let missing_context = context.missing_fields();
    if !missing_context.is_empty() {
        return Ok(empty_result(context.project_id.clone(), Readiness::ContextInsufficient, "CONTEXT_INSUFFICIENT", missing_context, corpus_evidence));
    }
    let discovery = discover(evidence)?;
    let roster = validated_roster(snapshot)?;
    let (required, recommended, excluded, bootstrap_actions, evidence_rows, provenance) = build_profile(context, &discovery, &roster);
    let agents: Vec<_> = required.iter().chain(recommended.iter()).filter(|item| item.capability_type == "ACTIVE_AGENT" && item.status == CapabilityStatus::MatchedExisting).cloned().collect();
    let required_capabilities: Vec<_> = required.iter().filter(|item| item.capability_type != "ACTIVE_AGENT").cloned().collect();
    let recommended_capabilities: Vec<_> = recommended.iter().filter(|item| item.capability_type != "ACTIVE_AGENT").cloned().collect();
    let gaps: Vec<_> = required
        .iter()
        .filter(|item| matches!(item.status, CapabilityStatus::Gap | CapabilityStatus::NoEvidence))
        .chain(recommended.iter().filter(|item| item.status == CapabilityStatus::Gap))
        .cloned()
        .collect();
    let referenced: BTreeSet<_> = required_capabilities.iter().chain(recommended_capabilities.iter()).map(|item| item.capability_id.as_str()).collect();
    let stale_states = discovery.iter().filter(|item| item.discovery_status == Some(DiscoveryState::Stale) && referenced.contains(item.id.as_str())).cloned().collect::<Vec<_>>();
    let approval_actions = bootstrap_actions.iter().filter(|action| action.action.ends_with("_REQUIRES_APPROVAL")).cloned().collect::<Vec<_>>();
    let warnings = !gaps.is_empty() || !stale_states.is_empty() || bootstrap_actions.iter().any(|a| matches!(a.action.as_str(), "VERIFY" | "CANONICALIZE" | "RESOLVE_STALE_STATE"));
    let next_operation = if !approval_actions.is_empty() { "REQUEST_APPROVAL" } else if warnings { "REVIEW_WARNINGS" } else { "CONTINUE_WORK" };
    Ok(PreparationResult {
        project_id: context.project_id.clone(), readiness: if warnings { Readiness::ReadyWithWarnings } else { Readiness::Ready }, context_status: "SUFFICIENT".into(),
        agents, required_capabilities, recommended_capabilities, excluded_capabilities: excluded, gaps, stale_states, approval_actions,
        bootstrap_actions, evidence: evidence_rows, provenance, missing_context: Vec::new(), next_operation: next_operation.into(), corpus_evidence,
    })
}

fn empty_result(project_id: String, readiness: Readiness, context_status: &str, missing: Vec<String>, corpus_evidence: CorpusEvidence) -> PreparationResult {
    PreparationResult { project_id, readiness, context_status: context_status.into(), agents: Vec::new(), required_capabilities: Vec::new(), recommended_capabilities: Vec::new(), excluded_capabilities: Vec::new(), gaps: Vec::new(), stale_states: Vec::new(), approval_actions: Vec::new(), bootstrap_actions: Vec::new(), evidence: Vec::new(), provenance: Vec::new(), missing_context: missing, next_operation: "COLLECT_CONTEXT".into(), corpus_evidence }
}

fn validated_roster(snapshot: &CorpusSnapshot) -> Result<Vec<RosterAgent>, RuntimeError> {
    let mut missing = Vec::new();
    let mut roster = Vec::new();
    for slug in RECONCILED_ROSTER_SLUGS {
        if let Some(entry) = snapshot.index.get(slug) {
            roster.push(RosterAgent { slug: slug.into(), name: entry.name.clone(), description: entry.description.clone() });
        } else { missing.push(slug); }
    }
    if !missing.is_empty() { return Err(RuntimeError::RosterIncomplete(missing.join(", "))); }
    Ok(roster)
}

fn missing_capability_action(required: bool) -> (&'static str, &'static str) {
    if required {
        (
            "INSTALL_REQUIRES_APPROVAL",
            "No existing registry entry or environment evidence — this would be a net-new capability, gated behind explicit human approval.",
        )
    } else {
        (
            "NO_ACTION",
            "No existing evidence, and not required for this project — not recommended from zero evidence.",
        )
    }
}

fn missing_operational_capability(
    behavior: MissingBehavior,
    required: bool,
) -> (CapabilityStatus, &'static str, &'static str, Option<String>) {
    if !required {
        return (
            CapabilityStatus::NoEvidence,
            "NO_ACTION",
            "Capability is relevant but optional, and no factual availability evidence was supplied.",
            None,
        );
    }

    if behavior == MissingBehavior::Gap {
        return (
            CapabilityStatus::Gap,
            "GAP",
            "Required workflow capability has no factual environment evidence; keep it visible as a gap.",
            None,
        );
    }

    let (action, reason) = missing_capability_action(true);
    (CapabilityStatus::NoEvidence, action, reason, None)
}

fn push_agent_requirement(
    roster: &[RosterAgent],
    required: &mut Vec<PreparedCapability>,
    recommended: &mut Vec<PreparedCapability>,
    actions: &mut Vec<BootstrapAction>,
    provenance: &mut Vec<String>,
    slug: &str,
    reason: &str,
    is_required: bool,
) {
    if required.iter().chain(recommended.iter()).any(|item| item.capability_id == slug) {
        return;
    }
    let Some(agent) = roster.iter().find(|agent| agent.slug == slug) else {
        return;
    };
    let item = PreparedCapability {
        capability_id: agent.slug.clone(),
        capability_type: "ACTIVE_AGENT".into(),
        reason: reason.into(),
        status: CapabilityStatus::MatchedExisting,
        evidence_ref: Some(format!("roster:{}", agent.slug)),
    };
    actions.push(BootstrapAction {
        capability_id: agent.slug.clone(),
        action: "USE_EXISTING".into(),
        reason: "Matched an existing reconciled agent for a declared project requirement.".into(),
    });
    provenance.push(format!("{}:df-authored-roster", agent.slug));
    if is_required { required.push(item); } else { recommended.push(item); }
}

fn push_agent_gap(
    required: &mut Vec<PreparedCapability>,
    recommended: &mut Vec<PreparedCapability>,
    actions: &mut Vec<BootstrapAction>,
    gap_id: &str,
    reason: &str,
    is_required: bool,
) {
    if required.iter().chain(recommended.iter()).any(|item| item.capability_id == gap_id) {
        return;
    }
    let item = PreparedCapability {
        capability_id: gap_id.into(),
        capability_type: "ACTIVE_AGENT".into(),
        reason: reason.into(),
        status: CapabilityStatus::Gap,
        evidence_ref: None,
    };
    actions.push(BootstrapAction {
        capability_id: gap_id.into(),
        action: "GAP".into(),
        reason: "No approved reconciled agent fits this requirement; no persona is improvised.".into(),
    });
    if is_required { required.push(item); } else { recommended.push(item); }
}

fn build_profile(context: &ProjectContext, discovery: &[DiscoveryRecord], roster: &[RosterAgent]) -> (Vec<PreparedCapability>, Vec<PreparedCapability>, Vec<PreparedCapability>, Vec<BootstrapAction>, Vec<EnvironmentObservation>, Vec<String>) {
    let fields = &context.project_context;
    let mut required = Vec::new();
    let mut recommended = Vec::new();
    let mut actions = Vec::new();
    let mut evidence_rows = Vec::new();
    let mut provenance = Vec::new();

    let signal_text = [
        fields.project_type.as_str(),
        fields.objective_or_problem.as_str(),
        &fields.deliverables.join(" "),
        &fields.scope.join(" "),
        &fields.existing_stack.join(" "),
        &fields.connected_services.join(" "),
        &fields.constraints.join(" "),
        &fields.existing_capabilities.join(" "),
    ].join(" ").to_lowercase();
    let has = |markers: &[&str]| markers.iter().any(|marker| signal_text.contains(marker));

    let web_project = has(&["website", "landing", "web page", "frontend", "html", "css", "responsive", "ui", "next.js"]);
    let cms_web = has(&["wordpress", "drupal"]);
    let custom_web = web_project && (has(&["static html", "html", "javascript", "next.js", "custom", "non-cms"]) || !cms_web);

    if web_project {
        push_agent_requirement(
            roster, &mut required, &mut recommended, &mut actions, &mut provenance,
            "design-system-foundation-agent",
            "The project declares a visual web/UI surface with layout, responsive, typography, or design-system needs.",
            true,
        );
    }

    if cms_web {
        push_agent_requirement(
            roster, &mut required, &mut recommended, &mut actions, &mut provenance,
            "engineering-web-implementation-agent",
            "The declared WordPress/Drupal stack matches the reconciled Web Implementation agent.",
            true,
        );
    } else if custom_web {
        push_agent_gap(
            &mut required, &mut recommended, &mut actions,
            "gap-custom-frontend",
            "The project requires static/custom frontend implementation, while the reconciled Web Implementation agent is scoped to Drupal/WordPress.",
            true,
        );
    }

    if has(&["motion", "micro-interaction", "interaction", "animation"]) {
        push_agent_requirement(
            roster, &mut required, &mut recommended, &mut actions, &mut provenance,
            "micro-interaction-delight-specialist",
            "The declared deliverables include motion or interaction behavior in the visual implementation.",
            false,
        );
    }

    if has(&["owner-review", "owner review", "design review", "finish gate", "visual review", "client review"]) {
        push_agent_requirement(
            roster, &mut required, &mut recommended, &mut actions, &mut provenance,
            "design-ui-finish-gate-reviewer",
            "The project is explicitly in an owner/client visual-review stage and benefits from a finish-gate review.",
            false,
        );
    }

    if has(&["landing", "conversion", "cta", "whatsapp", "marketing", "package configurator", "offer"]) {
        push_agent_requirement(
            roster, &mut required, &mut recommended, &mut actions, &mut provenance,
            "marketing-strategy-brief-agent",
            "The project has an offer/conversion objective that requires a clear execution brief and messaging boundary.",
            false,
        );
    }

    if has(&["copy", "content", "caption", "messaging", "headline", "offer"]) {
        push_agent_requirement(
            roster, &mut required, &mut recommended, &mut actions, &mut provenance,
            "marketing-content-copy-agent",
            "The project includes copy, content, captions, messaging, or offer language.",
            false,
        );
    }

    if has(&["browser qa", "selenium", "playwright", "visual qa", "qa evidence", "interaction qa"]) {
        push_agent_gap(
            &mut required, &mut recommended, &mut actions,
            "gap-browser-visual-qa",
            "Browser/visual QA is explicitly required, but no testing/QA agent is yet in the reconciled DF roster.",
            true,
        );
    }

    if has(&["accessibility qa", "accessibility", "wcag"]) {
        push_agent_gap(
            &mut required, &mut recommended, &mut actions,
            "gap-general-accessibility-review",
            "General accessibility review is required, but the reconciled roster only has a US Section 508-specific reviewer.",
            false,
        );
    }

    for capability in capability_registry::all() {
        let Some(is_required) = capability.requirement_for(&signal_text) else {
            continue;
        };

        let found = discovery
            .iter()
            .find(|record| record.id == capability.id);
        let (status, action, action_reason, evidence_ref) = match found {
            Some(record)
                if record.evidence_problem.is_some() && record.discovery_status.is_none() =>
            {
                (
                    CapabilityStatus::Verify,
                    "VERIFY",
                    "Evidence collection failed; do not infer absence.",
                    Some(format!("discovery:{}", capability.id)),
                )
            }
            Some(record) => match record.discovery_status {
                Some(DiscoveryState::Active) => (
                    CapabilityStatus::Active,
                    "NO_ACTION",
                    "Already active.",
                    Some(format!("discovery:{}", capability.id)),
                ),
                Some(DiscoveryState::Available) => {
                    let action = if capability.kind == "SERVICE" {
                        "CONNECT_REQUIRES_APPROVAL"
                    } else if capability.kind == "PLUGIN" {
                        "ENABLE_REQUIRES_APPROVAL"
                    } else {
                        "USE_EXISTING"
                    };
                    (
                        CapabilityStatus::Available,
                        action,
                        "Existing capability is available; any connection or enablement remains approval-gated.",
                        Some(format!("discovery:{}", capability.id)),
                    )
                }
                Some(DiscoveryState::Stale) => (
                    CapabilityStatus::Stale,
                    "RESOLVE_STALE_STATE",
                    "Registry and environment facts disagree; resolve before use.",
                    Some(format!("discovery:{}", capability.id)),
                ),
                Some(DiscoveryState::NeedsCanonicalization) => (
                    CapabilityStatus::Verify,
                    "CANONICALIZE",
                    "Canonical source is unresolved.",
                    Some(format!("discovery:{}", capability.id)),
                ),
                Some(DiscoveryState::Unclassified) => (
                    CapabilityStatus::Verify,
                    "VERIFY",
                    "Detected but not canonically classified.",
                    Some(format!("discovery:{}", capability.id)),
                ),
                Some(DiscoveryState::Missing)
                | Some(DiscoveryState::Registered)
                | Some(DiscoveryState::Detected)
                | None => missing_operational_capability(capability.missing_behavior, is_required),
            },
            None => missing_operational_capability(capability.missing_behavior, is_required),
        };

        let item = PreparedCapability {
            capability_id: capability.id.clone(),
            capability_type: capability.kind.clone(),
            reason: format!(
                "{} matched the normalized project context.",
                capability.label
            ),
            status,
            evidence_ref,
        };
        actions.push(BootstrapAction {
            capability_id: capability.id.clone(),
            action: action.into(),
            reason: action_reason.into(),
        });
        if let Some(record) = found {
            evidence_rows.extend(record.evidence.clone());
        }
        provenance.push(format!("{}:environment-evidence", capability.id));
        if is_required {
            required.push(item);
        } else {
            recommended.push(item);
        }
    }

    let excluded = [
        ("gstack", "WORKFLOW", "DO_NOT_INSTALL remains in force."),
        ("graphify", "CLI", "No canonical source or evidence exists."),
    ].into_iter().map(|(id, kind, reason)| PreparedCapability {
        capability_id: id.into(), capability_type: kind.into(), reason: reason.into(),
        status: CapabilityStatus::Gap, evidence_ref: None
    }).collect();

    (required, recommended, excluded, actions, evidence_rows, provenance)
}

fn validate_text(name: &str, value: &str, max: usize) -> Result<(), RuntimeError> {
    if value.len() > max || value.contains('\0') || value.contains(['\r', '\n']) {
        return Err(RuntimeError::InvalidProjectContext(format!("{name} is invalid or exceeds its bound")));
    }
    Ok(())
}

fn validate_timestamp(value: &str, name: &str) -> Result<(), RuntimeError> {
    if chrono::DateTime::parse_from_rfc3339(value).is_err() { return Err(RuntimeError::InvalidEnvironmentEvidence(format!("{name} is not RFC3339"))); }
    Ok(())
}

fn validate_capability_id(value: &str) -> Result<(), RuntimeError> {
    if value.is_empty() || value.len() > 128 || !value.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
        return Err(RuntimeError::InvalidEnvironmentEvidence("invalid capability_id".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CorpusMeta;

    fn context(id: &str) -> ProjectContext {
        ProjectContext { project_id: id.into(), project_context: ProjectContextFields {
            client_or_owner: "Thomas".into(), project_type: "client-project".into(), objective_or_problem: "Launch site".into(),
            deliverables: vec!["site".into()], scope: vec!["phase 1".into()], existing_stack: vec!["Next.js (custom, not a CMS)".into(), "Supabase".into(), "Resend".into()],
            connected_services: vec!["Supabase".into(), "Resend".into(), "Vercel".into()], constraints: vec!["legal".into()], approval_owner: "diogo".into(), existing_capabilities: vec!["git".into()],
        }}
    }

    fn evidence() -> EnvironmentEvidence {
        EnvironmentEvidence { evidence_schema_version: "1.0.0".into(), generated_at: "2026-09-21T10:00:00Z".into(), source_categories_consulted: vec![EvidenceSourceCategory::PluginRegistry, EvidenceSourceCategory::CliProbe], observations: vec![
            CapabilityObservations { capability_id: "vercel-cli".into(), observations: vec![
                EnvironmentObservation { source_category: EvidenceSourceCategory::PluginRegistry, kind: EvidenceKind::RegistryEntry, result: EvidenceResult::Absent, version: None, detail: None, observed_at: "2026-09-21T10:00:00Z".into() },
                EnvironmentObservation { source_category: EvidenceSourceCategory::CliProbe, kind: EvidenceKind::CliProbe, result: EvidenceResult::Present, version: Some("1.0".into()), detail: None, observed_at: "2026-09-21T10:00:00Z".into() },
            ]},
        ]}
    }

    fn snapshot() -> CorpusSnapshot {
        let index = RECONCILED_ROSTER_SLUGS.iter().map(|slug| (*slug, crate::types::CorpusEntry { slug: (*slug).into(), name: (*slug).into(), category: "engineering".into(), emoji: None, color: None, vibe: None, description: "generic capability fixture".into(), source_hash: "a".repeat(64), frontmatter_hash: "b".repeat(64), body_hash: "c".repeat(64) })).map(|(slug, entry)| (slug.into(), entry)).collect();
        CorpusSnapshot { index, meta: CorpusMeta { version: "baseline".into(), commit: None, fetched_at: "2026-09-21T09:00:00Z".into(), count: 26 }, provenance: "BASELINE", generation_id: "00000000-0000-0000-0000-000000000001".into(), generated_at: "2026-09-21T09:00:00Z".into() }
    }

    fn required_service_context(service: &str) -> ProjectContext {
        let mut value = context("fixture-project");
        value.project_context.connected_services = vec![service.into()];
        value.project_context.existing_stack = vec!["application stack".into()];
        value.project_context.existing_capabilities = vec!["existing capability".into()];
        value
    }

    fn evidence_with_git_present() -> EnvironmentEvidence {
        let mut value = evidence();
        value.observations.push(CapabilityObservations {
            capability_id: "git-cli".into(),
            observations: vec![EnvironmentObservation {
                source_category: EvidenceSourceCategory::CliProbe,
                kind: EvidenceKind::CliProbe,
                result: EvidenceResult::Present,
                version: Some("2.0".into()),
                detail: None,
                observed_at: "2026-09-21T10:00:00Z".into(),
            }],
        });
        value
    }

    #[test]
    fn strict_context_rejects_unknown_fields_and_reports_empty_context() {
        let raw = r#"{"project_id":"p1","project_context":{"client_or_owner":"","project_type":"","objective_or_problem":"","deliverables":[],"scope":[],"existing_stack":[],"connected_services":[],"constraints":[],"approval_owner":"","existing_capabilities":[]},"repoRoot":"x"}"#;
        assert!(serde_json::from_str::<ProjectContext>(raw).is_err());
        assert_eq!(context("p1").missing_fields(), Vec::<String>::new());
    }

    #[test]
    fn evidence_rejects_semantic_state_and_paths_are_not_a_field() {
        let raw = r#"{"evidence_schema_version":"1.0.0","generated_at":"2026-09-21T10:00:00Z","source_categories_consulted":["cli_probe"],"observations":[{"capability_id":"git-cli","observations":[{"source_category":"cli_probe","kind":"cli_probe","result":"STALE","observed_at":"2026-09-21T10:00:00Z"}]}]}"#;
        assert!(serde_json::from_str::<EnvironmentEvidence>(raw).is_err());
    }

    #[test]
    fn stale_and_probe_failure_remain_typed_without_false_missing() {
        let mut e = evidence();
        e.observations.push(CapabilityObservations { capability_id: "git-cli".into(), observations: vec![EnvironmentObservation { source_category: EvidenceSourceCategory::CliProbe, kind: EvidenceKind::CliProbe, result: EvidenceResult::ProbeFailed, version: None, detail: None, observed_at: "2026-09-21T10:00:00Z".into() }] });
        let result = discover(&e).unwrap();
        assert_eq!(result[0].discovery_status, Some(DiscoveryState::Stale));
        assert_eq!(result[1].discovery_status, None);
        assert!(result[1].evidence_problem.is_some());
    }

    #[test]
    fn preparation_keeps_probe_failure_as_verification_not_gap() {
        let mut e = evidence();
        e.observations.push(CapabilityObservations { capability_id: "git-cli".into(), observations: vec![EnvironmentObservation { source_category: EvidenceSourceCategory::CliProbe, kind: EvidenceKind::CliProbe, result: EvidenceResult::ProbeFailed, version: None, detail: None, observed_at: "2026-09-21T10:00:00Z".into() }] });
        let result = prepare_project(&context("atom-website"), &e, &snapshot()).unwrap();
        let git = result.required_capabilities.iter().find(|item| item.capability_id == "git-cli").unwrap();
        assert_eq!(git.status, CapabilityStatus::Verify);
        assert!(!result.gaps.iter().any(|item| item.capability_id == "git-cli"));
        assert!(result.bootstrap_actions.iter().any(|item| item.capability_id == "git-cli" && item.action == "VERIFY"));
    }

    #[test]
    fn static_landing_selects_reconciled_design_and_marketing_agents_but_keeps_real_gaps() {
        let mut c = context("static-landing-pilot");
        c.project_context.client_or_owner = "Internal Pilot".into();
        c.project_context.project_type = "partner landing page / static configurator".into();
        c.project_context.objective_or_problem =
            "Prepare a conversion landing page for owner review without inventing claims.".into();
        c.project_context.deliverables = vec![
            "responsive landing page".into(),
            "package configurator with CTA".into(),
            "motion and interaction QA".into(),
            "accessibility QA evidence".into(),
        ];
        c.project_context.scope = vec!["owner-review draft".into(), "static implementation".into()];
        c.project_context.existing_stack =
            vec!["Static HTML".into(), "CSS".into(), "JavaScript".into(), "Selenium browser QA".into()];
        c.project_context.connected_services = vec!["WhatsApp conversion link".into()];
        c.project_context.constraints =
            vec!["No deployment before owner approval".into(), "No fabricated claims".into()];
        c.project_context.existing_capabilities =
            vec!["git".into(), "browser QA".into(), "independent review".into()];

        let result = prepare_project(&c, &evidence_with_git_present(), &snapshot()).unwrap();

        let selected: BTreeSet<_> = result.agents.iter().map(|item| item.capability_id.as_str()).collect();
        assert!(selected.contains("design-system-foundation-agent"));
        assert!(selected.contains("micro-interaction-delight-specialist"));
        assert!(selected.contains("design-ui-finish-gate-reviewer"));
        assert!(selected.contains("marketing-strategy-brief-agent"));
        assert!(!selected.contains("engineering-web-implementation-agent"));

        assert!(result.gaps.iter().any(|item| item.capability_id == "gap-custom-frontend"));
        assert!(result.gaps.iter().any(|item| item.capability_id == "gap-browser-visual-qa"));
        assert!(result.gaps.iter().any(|item| item.capability_id == "gap-general-accessibility-review"));
        assert!(result.bootstrap_actions.iter().any(|item| {
            item.capability_id == "gap-custom-frontend" && item.action == "GAP"
        }));
    }

    #[test]
    fn vrc_video_offer_landing_selects_real_team_and_preserves_real_gaps() {
        let mut c = context("vrc-video-offer-landing");
        c.project_context.client_or_owner = "VRC Agency / owner review".into();
        c.project_context.project_type = "partner landing page / static package configurator".into();
        c.project_context.objective_or_problem =
            "Adapt the approved reference interaction into a VRC-branded conversion landing page without inheriting unsupported claims, prices, client assets, or deployment assumptions.".into();
        c.project_context.deliverables = vec![
            "responsive static landing page".into(),
            "single-project and quarterly package configurator".into(),
            "WhatsApp CTA payload".into(),
            "motion and micro-interactions".into(),
            "browser QA evidence".into(),
            "owner-review package".into(),
        ];
        c.project_context.scope = vec![
            "faithful adaptation".into(),
            "Digital Flow improved direction".into(),
            "owner-review draft".into(),
            "no production deploy".into(),
        ];
        c.project_context.existing_stack = vec![
            "Static HTML".into(),
            "CSS".into(),
            "JavaScript".into(),
            "Selenium browser QA".into(),
        ];
        c.project_context.connected_services = vec![
            "WhatsApp conversion link".into(),
            "Instagram public profile".into(),
        ];
        c.project_context.constraints = vec![
            "No deploy or indexing before owner approval".into(),
            "No inherited Natan commercial values or assets".into(),
            "No fabricated ROI, audience, savings, or testimonial claims".into(),
            "Commercial values, payment terms, VRC videos, coverage, validity, and Gotham webfont remain owner inputs".into(),
        ];
        c.project_context.approval_owner = "Diogo".into();
        c.project_context.existing_capabilities = vec![
            "git".into(),
            "browser QA".into(),
            "independent review".into(),
            "df-website-delivery".into(),
        ];

        let result = prepare_project(&c, &evidence_with_git_present(), &snapshot()).unwrap();

        assert_eq!(result.project_id, "vrc-video-offer-landing");
        assert_eq!(result.context_status, "SUFFICIENT");
        let selected: BTreeSet<_> = result
            .agents
            .iter()
            .map(|item| item.capability_id.as_str())
            .collect();
        assert!(selected.contains("design-system-foundation-agent"));
        assert!(selected.contains("micro-interaction-delight-specialist"));
        assert!(selected.contains("design-ui-finish-gate-reviewer"));
        assert!(selected.contains("marketing-strategy-brief-agent"));
        assert!(!selected.contains("engineering-web-implementation-agent"));

        assert!(result
            .gaps
            .iter()
            .any(|item| item.capability_id == "gap-custom-frontend"));
        assert!(result
            .gaps
            .iter()
            .any(|item| item.capability_id == "gap-browser-visual-qa"));

        let node = result
            .recommended_capabilities
            .iter()
            .find(|item| item.capability_id == "node-runtime")
            .expect("JavaScript should map generically to Node.js runtime support");
        assert_eq!(node.status, CapabilityStatus::NoEvidence);
        let browser_qa = result
            .recommended_capabilities
            .iter()
            .find(|item| item.capability_id == "playwright-browser-qa")
            .expect("browser QA should map generically to Playwright support");
        assert_eq!(browser_qa.status, CapabilityStatus::NoEvidence);
        assert!(!result
            .gaps
            .iter()
            .any(|item| item.capability_id == "playwright-browser-qa"));

        assert_eq!(result.next_operation, "REVIEW_WARNINGS");
        assert!(result.approval_actions.is_empty());
    }

    #[test]
    fn factual_present_evidence_changes_registry_matched_capability_to_available() {
        let mut c = context("browser-qa-proof");
        c.project_context.existing_stack = vec!["Playwright".into()];
        c.project_context.connected_services = vec!["local browser QA".into()];
        c.project_context.existing_capabilities = vec!["git".into()];

        let mut e = evidence_with_git_present();
        e.observations.push(CapabilityObservations {
            capability_id: "playwright-browser-qa".into(),
            observations: vec![
                EnvironmentObservation {
                    source_category: EvidenceSourceCategory::CapabilityRegistry,
                    kind: EvidenceKind::RegistryEntry,
                    result: EvidenceResult::Present,
                    version: None,
                    detail: None,
                    observed_at: "2026-09-21T10:00:00Z".into(),
                },
                EnvironmentObservation {
                    source_category: EvidenceSourceCategory::CliProbe,
                    kind: EvidenceKind::CliProbe,
                    result: EvidenceResult::Present,
                    version: Some("1.57.0".into()),
                    detail: None,
                    observed_at: "2026-09-21T10:00:00Z".into(),
                },
            ],
        });

        let result = prepare_project(&c, &e, &snapshot()).unwrap();
        let playwright = result
            .required_capabilities
            .iter()
            .find(|item| item.capability_id == "playwright-browser-qa")
            .expect("explicit Playwright stack should require the capability");
        assert_eq!(playwright.status, CapabilityStatus::Available);
        assert!(!result
            .gaps
            .iter()
            .any(|item| item.capability_id == "playwright-browser-qa"));
        assert!(result.bootstrap_actions.iter().any(|item| {
            item.capability_id == "playwright-browser-qa" && item.action == "USE_EXISTING"
        }));
    }

    #[test]
    fn atom_preparation_preserves_gap_stale_and_approval_boundary() {
        let result = prepare_project(&context("atom-website"), &evidence_with_git_present(), &snapshot()).unwrap();
        assert_eq!(result.readiness, Readiness::ReadyWithWarnings);
        assert!(result.gaps.iter().any(|item| item.capability_id == "gap-custom-frontend"));
        assert!(result.stale_states.iter().any(|item| item.id == "vercel-cli"));
        assert!(result.bootstrap_actions.iter().any(|item| item.action == "RESOLVE_STALE_STATE"));
        assert_eq!(result.approval_actions.iter().filter(|item| item.action == "INSTALL_REQUIRES_APPROVAL").count(), 2);
        assert!(result.bootstrap_actions.iter().filter(|item| item.action == "INSTALL_REQUIRES_APPROVAL").count() == 2);
        assert_eq!(result.next_operation, "REQUEST_APPROVAL");
    }

    #[test]
    fn required_no_evidence_generates_visible_install_approval() {
        let result = prepare_project(&required_service_context("Supabase"), &evidence(), &snapshot()).unwrap();
        let capability = result.required_capabilities.iter().find(|item| item.capability_id == "supabase-service").unwrap();
        assert_eq!(capability.status, CapabilityStatus::NoEvidence);
        assert_eq!(result.approval_actions.iter().filter(|item| item.capability_id == "supabase-service").count(), 1);
        assert!(result.bootstrap_actions.iter().any(|item| item.capability_id == "supabase-service" && item.action == "INSTALL_REQUIRES_APPROVAL"));
        assert_eq!(result.next_operation, "REQUEST_APPROVAL");
    }

    #[test]
    fn required_canonical_missing_generates_install_approval_without_installing() {
        let mut environment = evidence();
        environment.observations.push(CapabilityObservations {
            capability_id: "supabase-service".into(),
            observations: vec![EnvironmentObservation {
                source_category: EvidenceSourceCategory::CapabilityRegistry,
                kind: EvidenceKind::RegistryEntry,
                result: EvidenceResult::Present,
                version: None,
                detail: None,
                observed_at: "2026-09-21T10:00:00Z".into(),
            }],
        });
        let result = prepare_project(&required_service_context("Supabase"), &environment, &snapshot()).unwrap();
        let capability = result.required_capabilities.iter().find(|item| item.capability_id == "supabase-service").unwrap();
        assert_eq!(capability.status, CapabilityStatus::NoEvidence);
        assert!(result.approval_actions.iter().any(|item| item.capability_id == "supabase-service" && item.action == "INSTALL_REQUIRES_APPROVAL"));
        assert!(!result.bootstrap_actions.iter().any(|item| item.action == "INSTALL"));
    }

    #[test]
    fn optional_no_evidence_does_not_inherit_required_install_approval() {
        assert_eq!(missing_capability_action(false).0, "NO_ACTION");
        assert_ne!(missing_capability_action(false).0, "INSTALL_REQUIRES_APPROVAL");
    }

    #[test]
    fn publicflow_is_context_insufficient_before_capability_assessment() {
        let mut c = context("publicflow-secure-ops");
        c.project_context.deliverables.clear();
        let result = prepare_project(&c, &evidence(), &snapshot()).unwrap();
        assert_eq!(result.readiness, Readiness::ContextInsufficient);
        assert!(result.gaps.is_empty());
        assert!(result.missing_context.contains(&"deliverables".to_string()));
    }

    #[test]
    fn project_ids_isolate_preparation_results() {
        let first = prepare_project(&context("atom-website"), &evidence(), &snapshot()).unwrap();
        let second = prepare_project(&context("another-project"), &evidence(), &snapshot()).unwrap();
        assert_eq!(first.project_id, "atom-website");
        assert_eq!(second.project_id, "another-project");
    }

    #[test]
    fn agent_agency_is_a_project_context_not_a_recursive_resolution_request() {
        let result = prepare_project(&context("agent-agency"), &evidence(), &snapshot()).unwrap();
        assert_eq!(result.project_id, "agent-agency");
        assert_ne!(result.project_id, "agent-agency/agent-agency");
    }

    #[test]
    fn incomplete_roster_fails_closed_before_preparation() {
        let mut incomplete = snapshot();
        incomplete.index.remove("engineering-web-implementation-agent");
        let error = prepare_project(&context("atom-website"), &evidence(), &incomplete).unwrap_err();
        assert!(matches!(error, RuntimeError::RosterIncomplete(slug) if slug == "engineering-web-implementation-agent"));
    }
}
