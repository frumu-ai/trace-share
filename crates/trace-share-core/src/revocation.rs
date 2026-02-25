use anyhow::Result;
use chrono::Utc;

use crate::{config::AppConfig, state::StateStore, worker::push_revocation};

pub fn revoke_local(store: &StateStore, episode_id: &str, reason: Option<&str>) -> Result<()> {
    store.upsert_revocation(episode_id, reason, &Utc::now().to_rfc3339(), "pending")
}

pub async fn sync_revocations(config: &AppConfig, store: &StateStore) -> Result<usize> {
    let pending = store.pending_revocations()?;
    let mut pushed = 0usize;

    for item in pending {
        push_revocation(
            config,
            &item.episode_id,
            &item.revoked_at,
            item.reason.as_deref(),
        )
        .await?;
        store.mark_revocation_pushed(&item.episode_id)?;
        pushed += 1;
    }

    Ok(pushed)
}
