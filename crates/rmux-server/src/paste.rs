//! Paste buffer storage.
//!
//! Provides a global stack of paste buffers with most-recent-first ordering.
//! Buffers can be automatically named (from copy operations) or explicitly named.

use std::collections::VecDeque;

/// A single paste buffer entry.
#[derive(Debug, Clone)]
pub struct PasteBuffer {
    /// Buffer name (e.g., "buffer0000" for automatic, or user-defined).
    pub name: String,
    /// Buffer data.
    pub data: Vec<u8>,
    /// Whether this buffer was automatically created (vs. explicitly named).
    pub automatic: bool,
}

/// Global paste buffer storage.
///
/// Buffers are ordered most-recent-first. Automatic buffers are trimmed
/// when the count exceeds the configured limit.
#[derive(Debug)]
pub struct PasteBufferStore {
    /// Buffers ordered by creation time (index 0 = most recent).
    buffers: VecDeque<PasteBuffer>,
    /// Maximum number of automatic buffers.
    limit: usize,
    /// Next automatic buffer index.
    next_index: u32,
}

impl PasteBufferStore {
    /// Create a new paste buffer store with the given limit.
    pub fn new(limit: usize) -> Self {
        Self { buffers: VecDeque::new(), limit, next_index: 0 }
    }

    /// Add data as an automatically-named buffer.
    ///
    /// The buffer is named "buffer0000", "buffer0001", etc.
    /// If the number of automatic buffers exceeds the limit, the oldest
    /// automatic buffer is removed.
    pub fn add(&mut self, data: Vec<u8>) {
        let name = format!("buffer{:04}", self.next_index);
        self.next_index += 1;

        self.buffers.push_front(PasteBuffer { name, data, automatic: true });

        // Enforce limit on automatic buffers
        self.enforce_limit();
    }

    /// Set a named buffer's contents. Creates or replaces the buffer.
    pub fn set(&mut self, name: &str, data: Vec<u8>) {
        // Remove existing buffer with this name
        self.buffers.retain(|b| b.name != name);

        // Use the provided name or auto-generate one
        let buf_name = if name.is_empty() {
            let n = format!("buffer{:04}", self.next_index);
            self.next_index += 1;
            n
        } else {
            name.to_string()
        };

        self.buffers.push_front(PasteBuffer { name: buf_name, data, automatic: name.is_empty() });

        if name.is_empty() {
            self.enforce_limit();
        }
    }

    /// Get the most recently created buffer.
    pub fn get_top(&self) -> Option<&PasteBuffer> {
        self.buffers.front()
    }

    /// Get a buffer by name.
    pub fn get_by_name(&self, name: &str) -> Option<&PasteBuffer> {
        self.buffers.iter().find(|b| b.name == name)
    }

    /// Delete a buffer by name. Returns true if found and removed.
    pub fn delete(&mut self, name: &str) -> bool {
        let len = self.buffers.len();
        self.buffers.retain(|b| b.name != name);
        self.buffers.len() < len
    }

    /// List all buffers (most-recent-first).
    pub fn list(&self) -> Vec<&PasteBuffer> {
        self.buffers.iter().collect()
    }

    /// Clear all buffers.
    pub fn clear(&mut self) {
        self.buffers.clear();
    }

    /// Number of buffers.
    pub fn len(&self) -> usize {
        self.buffers.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }

    /// Trim oldest automatic buffers if over limit.
    fn enforce_limit(&mut self) {
        let auto_count = self.buffers.iter().filter(|b| b.automatic).count();
        if auto_count > self.limit {
            let excess = auto_count - self.limit;
            let mut removed = 0;
            // Iterate from back (oldest) and mark indices to remove
            let mut indices_to_remove = Vec::new();
            for i in (0..self.buffers.len()).rev() {
                if removed >= excess {
                    break;
                }
                if self.buffers[i].automatic {
                    indices_to_remove.push(i);
                    removed += 1;
                }
            }
            // Remove in reverse order (highest index first) to preserve indices
            for i in indices_to_remove {
                self.buffers.remove(i);
            }
        }
    }
}

impl Default for PasteBufferStore {
    fn default() -> Self {
        Self::new(50)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_get_top() {
        let mut store = PasteBufferStore::new(50);
        store.add(b"hello".to_vec());
        let top = store.get_top().unwrap();
        assert_eq!(top.data, b"hello");
        assert_eq!(top.name, "buffer0000");
        assert!(top.automatic);
    }

    #[test]
    fn most_recent_first() {
        let mut store = PasteBufferStore::new(50);
        store.add(b"first".to_vec());
        store.add(b"second".to_vec());
        store.add(b"third".to_vec());

        let top = store.get_top().unwrap();
        assert_eq!(top.data, b"third");

        let all = store.list();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].data, b"third");
        assert_eq!(all[1].data, b"second");
        assert_eq!(all[2].data, b"first");
    }

    #[test]
    fn limit_enforcement() {
        let mut store = PasteBufferStore::new(3);
        store.add(b"a".to_vec());
        store.add(b"b".to_vec());
        store.add(b"c".to_vec());
        assert_eq!(store.len(), 3);

        store.add(b"d".to_vec());
        assert_eq!(store.len(), 3);
        // Oldest should be removed
        assert!(store.get_by_name("buffer0000").is_none());
        assert!(store.get_by_name("buffer0003").is_some());
    }

    #[test]
    fn named_buffer_not_limited() {
        let mut store = PasteBufferStore::new(2);
        store.set("custom", b"x".to_vec());
        store.add(b"a".to_vec());
        store.add(b"b".to_vec());
        store.add(b"c".to_vec()); // This should evict oldest automatic, not "custom"

        assert!(store.get_by_name("custom").is_some());
        assert_eq!(store.len(), 3); // custom + 2 automatic
    }

    #[test]
    fn get_by_name() {
        let mut store = PasteBufferStore::new(50);
        store.add(b"first".to_vec());
        store.add(b"second".to_vec());

        let buf = store.get_by_name("buffer0000").unwrap();
        assert_eq!(buf.data, b"first");

        let buf = store.get_by_name("buffer0001").unwrap();
        assert_eq!(buf.data, b"second");
    }

    #[test]
    fn delete_buffer() {
        let mut store = PasteBufferStore::new(50);
        store.add(b"hello".to_vec());
        assert!(store.delete("buffer0000"));
        assert!(store.is_empty());
        assert!(!store.delete("nonexistent"));
    }

    #[test]
    fn set_replaces_existing() {
        let mut store = PasteBufferStore::new(50);
        store.set("test", b"old".to_vec());
        store.set("test", b"new".to_vec());
        assert_eq!(store.len(), 1);
        assert_eq!(store.get_by_name("test").unwrap().data, b"new");
    }

    #[test]
    fn set_empty_name_auto_generates() {
        let mut store = PasteBufferStore::new(50);
        store.set("", b"data".to_vec());
        let top = store.get_top().unwrap();
        assert!(top.name.starts_with("buffer"));
        assert!(top.automatic);
    }

    #[test]
    fn clear_removes_all() {
        let mut store = PasteBufferStore::new(50);
        store.add(b"a".to_vec());
        store.add(b"b".to_vec());
        store.clear();
        assert!(store.is_empty());
    }
}
