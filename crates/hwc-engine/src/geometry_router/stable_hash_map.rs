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

#[cfg(test)]
#[allow(unwrap_used, expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut map = StableHashMap::new();
        map.insert(3, "three");
        map.insert(1, "one");
        map.insert(2, "two");

        assert_eq!(map.get(&1), Some(&"one"));
        assert_eq!(map.get(&2), Some(&"two"));
        assert_eq!(map.get(&3), Some(&"three"));
        assert_eq!(map.get(&4), None);
    }

    #[test]
    fn test_insert_returns_previous() {
        let mut map = StableHashMap::new();
        assert_eq!(map.insert(1, "a"), None);
        assert_eq!(map.insert(1, "b"), Some("a"));
    }

    #[test]
    fn test_iter_deterministic_returns_sorted() {
        let mut map = StableHashMap::new();
        map.insert(50, "fifty");
        map.insert(10, "ten");
        map.insert(30, "thirty");
        map.insert(20, "twenty");

        let entries = map.iter_deterministic();
        let keys: Vec<&i32> = entries.iter().map(|(k, _)| k).copied().collect();
        assert_eq!(keys, vec![&10, &20, &30, &50]);
    }

    #[test]
    fn test_two_iterations_same_order() {
        let mut map: StableHashMap<u64, u64> = StableHashMap::new();
        for i in 0u64..100 {
            map.insert(i * 7 % 97, i);
        }

        let order1: Vec<(u64, u64)> = map
            .iter_deterministic()
            .into_iter()
            .map(|(k, v)| (*k, *v))
            .collect();
        let order2: Vec<(u64, u64)> = map
            .iter_deterministic()
            .into_iter()
            .map(|(k, v)| (*k, *v))
            .collect();
        assert_eq!(order1, order2);
    }

    #[test]
    fn test_keys_sorted() {
        let mut map = StableHashMap::new();
        map.insert("c", 3);
        map.insert("a", 1);
        map.insert("b", 2);

        let keys = map.keys_sorted();
        assert_eq!(keys, vec![&"a", &"b", &"c"]);
    }

    #[test]
    fn test_len_and_is_empty() {
        let mut map = StableHashMap::new();
        assert!(map.is_empty());
        assert_eq!(map.len(), 0);

        map.insert(1, "one");
        assert!(!map.is_empty());
        assert_eq!(map.len(), 1);

        map.insert(2, "two");
        assert_eq!(map.len(), 2);

        map.insert(1, "one_again");
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn test_contains_key() {
        let mut map = StableHashMap::new();
        map.insert(10, "ten");

        assert!(map.contains_key(&10));
        assert!(!map.contains_key(&20));
    }

    #[test]
    fn test_deterministic_seed_constant() {
        assert_eq!(DETERMINISTIC_SEED, 0x5EED_2024_0000_0001);
    }

    #[test]
    fn test_hash_set_basics() {
        let mut set = StableHashSet::new();
        assert!(set.insert(5));
        assert!(set.insert(3));
        assert!(!set.insert(5));

        assert!(set.contains(&3));
        assert!(!set.contains(&7));
        assert_eq!(set.len(), 2);
        assert!(!set.is_empty());
    }

    #[test]
    fn test_hash_set_iter_deterministic() {
        let mut set = StableHashSet::new();
        set.insert(50);
        set.insert(10);
        set.insert(30);
        set.insert(20);

        let values = set.iter_deterministic();
        assert_eq!(values, vec![&10, &20, &30, &50]);
    }

    #[test]
    fn test_default_implementations() {
        let mut map: StableHashMap<u64, u64> = StableHashMap::default();
        map.insert(1, 100);
        assert_eq!(map.get(&1), Some(&100));

        let mut set: StableHashSet<u64> = StableHashSet::default();
        set.insert(42);
        assert!(set.contains(&42));
    }
}
