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

pub async fn resolve_server_ip(state: &AppState, session_id: &Uuid) -> Result<String> {
    if let Some(server_url) = CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        cache.get(session_id).cloned()
    }) {
        return Ok(server_url);
    }

    let mut redis = state.redis.lock().await;
    let server_ip: String = redis.get(format!("session:{}", session_id)).await?;
    CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        cache.push(session_id.clone(), server_ip.clone());
    });

    Ok(server_ip)
}
