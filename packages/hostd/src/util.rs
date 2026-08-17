//! Small crate-wide helpers shared by `protocol` and `application`.
//!
//! Kept intentionally tiny: not every pure helper needs to become a port.
//! Neither `protocol`
//! nor `application` may depend on the other, so shared leaf helpers live
//! here at the crate root instead.

use crate::api::{ProtocolError, ServerMessage};
use crate::infra::storage::SessionStorageError;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;

pub(crate) type ClientEventSender =
    piko_comms::MailboxSender<piko_comms::contracts::HostCommandOutput, ServerMessage>;
pub(crate) type ClientEventReceiver =
    piko_comms::MailboxReceiver<piko_comms::contracts::HostCommandOutput, ServerMessage>;

pub(crate) async fn send_event(tx: &ClientEventSender, event: ServerMessage) {
    let _ = tx.send(event).await;
}

pub(crate) fn storage_error(error: SessionStorageError) -> ProtocolError {
    ProtocolError::InvalidCommand(error.to_string())
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Capacity-bounded least-recently-used map for process caches. Not
/// thread-safe; callers wrap it in a `Mutex`. Inserting beyond capacity
/// evicts the least-recently-touched entry.
#[derive(Debug)]
pub(crate) struct LruMap<K, V> {
    entries: HashMap<K, (u64, V)>,
    next_seq: u64,
    capacity: usize,
}

impl<K: Eq + Hash + Clone, V> LruMap<K, V> {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            next_seq: 0,
            capacity,
        }
    }

    pub(crate) fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let entry = self.entries.get_mut(key)?;
        self.next_seq += 1;
        entry.0 = self.next_seq;
        Some(&entry.1)
    }

    pub(crate) fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let entry = self.entries.get_mut(key)?;
        self.next_seq += 1;
        entry.0 = self.next_seq;
        Some(&mut entry.1)
    }

    pub(crate) fn insert(&mut self, key: K, value: V) {
        if self.capacity == 0 {
            return;
        }
        if !self.entries.contains_key(&key) && self.entries.len() >= self.capacity {
            let oldest_key = self
                .entries
                .iter()
                .min_by_key(|(_, (seq, _))| *seq)
                .map(|(key, _)| (*key).clone());
            if let Some(oldest) = oldest_key {
                self.entries.remove(&oldest);
            }
        }
        self.next_seq += 1;
        self.entries.insert(key, (self.next_seq, value));
    }

    pub(crate) fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.entries.remove(key).map(|(_, value)| value)
    }
}

#[cfg(test)]
mod tests {
    use super::LruMap;

    #[test]
    fn lru_evicts_least_recently_used() {
        let mut cache = LruMap::new(2);
        cache.insert("a", 1);
        cache.insert("b", 2);
        // Touch "a" so "b" becomes the least-recently-used entry.
        assert_eq!(cache.get("a"), Some(&1));
        cache.insert("c", 3);
        assert_eq!(cache.get("a"), Some(&1));
        assert_eq!(cache.get("b"), None);
        assert_eq!(cache.get("c"), Some(&3));
        // Removal works and frees a slot.
        assert_eq!(cache.remove("a"), Some(1));
        cache.insert("d", 4);
        assert_eq!(cache.get("c"), Some(&3));
        assert_eq!(cache.get("d"), Some(&4));
        assert_eq!(cache.get("a"), None);
    }
}
