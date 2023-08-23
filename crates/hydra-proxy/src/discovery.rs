use lru::LruCache;
use redis::AsyncCommands;
use shared::prelude::*;
use std::{cell::RefCell, num::NonZeroUsize};
use uuid::Uuid;

use crate::AppState;

thread_local! {
    static CACHE: RefCell<LruCache<Uuid, String>> = RefCell::new(LruCache::new(
        NonZeroUsize::new(10000).expect("enter anything but 0"),
    ));
}

async fn resolve_server_ip(state: &AppState, session_id: &Uuid) -> Result<Option<String>> {
    if let Some(server_url) = CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        cache.get(session_id).cloned()
    }) {
        return Ok(Some(server_url));
    }

    let mut redis = state.redis.lock().await;
    let server_ip: Option<String> = redis.get(format!("session:{}", session_id)).await?;
    if let Some(server_ip) = server_ip.clone() {
        CACHE.with(|c| {
            let mut cache = c.borrow_mut();
            cache.push(session_id.clone(), server_ip.clone());
        });
        return Ok(Some(server_ip));
    }

    Ok(server_ip)
}

pub async fn resolve_server_authority(
    state: &AppState,
    session_id: &Uuid,
) -> Result<Option<String>> {
    let server_ip = match resolve_server_ip(state, session_id).await? {
        Some(server_ip) => server_ip,
        None => return Ok(None),
    };

    let is_ipv6 = server_ip.contains(':');

    if is_ipv6 {
        let ip = format!("[{server_ip}]:3100");
        log::info!("proxying to {ip}");
        Ok(Some(ip))
    } else {
        Ok(Some(format!("{server_ip}:3100")))
    }
}
