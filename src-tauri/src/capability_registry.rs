//! Operational capability registry for project preparation.
//!
//! This is deliberately separate from `registry.rs` / `data/tools.json`.
//! The tool registry describes agent-consumer install targets. This registry
//! describes project capabilities. A registry entry is taxonomy only: it never
//! proves that a capability is available on the current machine.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use serde::Deserialize;

const CAPABILITIES_JSON: &str = include_str!("../data/capabilities.json");

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MissingBehavior {
    InstallApproval,
    ConnectApproval,
    Gap,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityMeta {
    pub id: String,
    pub label: String,
    pub kind: String,
    #[serde(default)]
    pub required_markers: Vec<String>,
    #[serde(default)]
    pub recommended_markers: Vec<String>,
    pub missing_behavior: MissingBehavior,
}

#[derive(Deserialize)]
struct Catalog {
    capabilities: Vec<CapabilityMeta>,
}

fn registry() -> &'static Vec<CapabilityMeta> {
    static REG: OnceLock<Vec<CapabilityMeta>> = OnceLock::new();
    REG.get_or_init(|| {
        let catalog: Catalog = serde_json::from_str(CAPABILITIES_JSON)
            .unwrap_or_else(|error| panic!("invalid capabilities.json: {error}"));
        let mut ids = BTreeSet::new();
        for capability in &catalog.capabilities {
            assert!(
                ids.insert(capability.id.as_str()),
                "duplicate operational capability id: {}",
                capability.id
            );
        }
        catalog.capabilities
    })
}

pub fn all() -> &'static [CapabilityMeta] {
    registry().as_slice()
}

fn normalized_tokens(value: &str) -> BTreeSet<String> {
    value
        .to_lowercase()
        .split(|character: char| {
            !character.is_ascii_alphanumeric() && character != '.' && character != '-'
        })
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub fn marker_matches(signal_text: &str, marker: &str) -> bool {
    let marker = marker.trim().to_lowercase();
    if marker.is_empty() {
        return false;
    }
    if marker.contains(' ') {
        return signal_text.to_lowercase().contains(&marker);
    }
    normalized_tokens(signal_text).contains(&marker)
}

impl CapabilityMeta {
    pub fn requirement_for(&self, signal_text: &str) -> Option<bool> {
        if self
            .required_markers
            .iter()
            .any(|marker| marker_matches(signal_text, marker))
        {
            return Some(true);
        }
        if self
            .recommended_markers
            .iter()
            .any(|marker| marker_matches(signal_text, marker))
        {
            return Some(false);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_parses_with_unique_ids_and_expected_milestone_capabilities() {
        let ids: BTreeSet<_> = all().iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids.len(), all().len());
        for id in [
            "git-cli",
            "node-runtime",
            "npm-tooling",
            "python-runtime",
            "ffmpeg",
            "docker",
            "vercel-cli",
            "supabase-service",
            "resend-service",
            "playwright-browser-qa",
            "remotion-video",
            "capcut-video-editing",
            "social-content-production",
            "image-design-production",
            "static-web-implementation",
        ] {
            assert!(ids.contains(id), "missing capability {id}");
        }
    }

    #[test]
    fn short_markers_are_token_matched_not_substring_matched() {
        assert!(marker_matches("Git repository", "git"));
        assert!(!marker_matches("Digital Flow", "git"));
    }
}
