use anyhow::{Result, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::state::StateStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentState {
    pub accepted_at: String,
    pub consent_version: String,
    pub license: String,
    pub public_searchable: bool,
    pub trainable: bool,
    pub ack_sanitization: bool,
    pub ack_public_search: bool,
    pub ack_training_release: bool,
}

pub fn allowed_license(license: &str) -> bool {
    matches!(license, "CC0-1.0" | "CC-BY-4.0")
}

pub fn init_consent(
    store: &StateStore,
    license: &str,
    consent_version: &str,
) -> Result<ConsentState> {
    if !allowed_license(license) {
        bail!("license must be one of: CC0-1.0, CC-BY-4.0");
    }

    let state = ConsentState {
        accepted_at: Utc::now().to_rfc3339(),
        consent_version: consent_version.to_string(),
        license: license.to_string(),
        public_searchable: true,
        trainable: true,
        ack_sanitization: true,
        ack_public_search: true,
        ack_training_release: true,
    };
    store.upsert_consent_state(&state)?;
    Ok(state)
}

pub fn require_consent(store: &StateStore) -> Result<ConsentState> {
    let Some(state) = store.consent_state()? else {
        bail!(
            "consent not initialized. run: trace-share consent init --license <CC0-1.0|CC-BY-4.0>"
        );
    };
    if !state.public_searchable || !state.trainable {
        bail!("consent state does not allow searchable+trainable uploads");
    }
    if !allowed_license(&state.license) {
        bail!("consent license invalid: {}", state.license);
    }
    if !(state.ack_sanitization && state.ack_public_search && state.ack_training_release) {
        bail!("consent acknowledgements incomplete");
    }
    Ok(state)
}
