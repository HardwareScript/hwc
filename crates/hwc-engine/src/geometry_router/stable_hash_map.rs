use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// Deterministic seed constant for the HWC engine.
pub const DETERMINISTIC_SEED: u64 = 0x5EED_2024_0000_0001;

/// A hash map wrapper that provides deterministic iteration order.
///
/// Internally uses [`HashMap`] for O(1) lookups, but iteration via
/// [`iter_deterministic`](StableHashMap::iter_deterministic) and
/// [`keys_sorted`](StableHashMap::keys_sorted) always returns entries
/// sorted by key.
pub struct StableHashMap<K, V> {
    inner: HashMap<K, V>,
}

impl<K: Hash + Eq + Ord, V> StableHashMap<K, V> {
    /// Create an empty map with the default hasher.
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    /// Insert a key-value pair, returning the previous value if the key existed.
    #[inline]
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.inner.insert(key, value)
    }

    /// Get a reference to the value for the given key.
    #[inline]
    pub fn get(&self, key: &K) -> Option<&V> {
        self.inner.get(key)
    }

    /// Return all entries sorted by key for deterministic iteration.
    pub fn iter_deterministic(&self) -> Vec<(&K, &V)> {
        let mut entries: Vec<(&K, &V)> = self.inner.iter().collect();
        entries.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));
        entries
    }

    /// Return the number of entries.
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the map contains no entries.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns `true` if the map contains the given key.
    #[inline]
    pub fn contains_key(&self, key: &K) -> bool {
        self.inner.contains_key(key)
    }

    /// Return all keys sorted in deterministic order.
    pub fn keys_sorted(&self) -> Vec<&K> {
        let mut keys: Vec<&K> = self.inner.keys().collect();
        keys.sort();
        keys
    }
}

impl<K: Hash + Eq + Ord, V> Default for StableHashMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

/// A hash set wrapper that provides deterministic iteration order.
pub struct StableHashSet<K> {
    inner: HashSet<K>,
}

impl<K: Hash + Eq + Ord> StableHashSet<K> {
    /// Create an empty set with the default hasher.
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: HashSet::new(),
        }
    }

    /// Insert a value, returning `true` if the value was not already present.
    #[inline]
    pub fn insert(&mut self, key: K) -> bool {
        self.inner.insert(key)
    }

    /// Returns `true` if the set contains the given value.
    #[inline]
    pub fn contains(&self, key: &K) -> bool {
        self.inner.contains(key)
    }

    /// Return all values sorted in deterministic order.
    pub fn iter_deterministic(&self) -> Vec<&K> {
        let mut keys: Vec<&K> = self.inner.iter().collect();
        keys.sort();
        keys
    }

    /// Return the number of values.
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the set contains no values.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl<K: Hash + Eq + Ord> Default for StableHashSet<K> {
    fn default() -> Self {
        Self::new()
    }
}
