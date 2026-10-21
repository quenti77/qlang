//! Small string-keyed maps for the interpreter's variables and struct fields.
//!
//! Most scopes hold a handful of variables and most structs a handful of fields, so a
//! vector searched linearly beats a hash table (no hashing, no table to allocate), and
//! names short enough to fit inline need no allocation at all. Past [`INDEX_FROM`]
//! entries a hash index is added so big scopes (a program's top level) stay fast.

use crate::fx::FxHashMap;
use std::borrow::Borrow;
use std::hash::{Hash, Hasher};

const INLINE: usize = 22;
const INDEX_FROM: usize = 12;

/// A name: stored in place when short, boxed otherwise.
#[derive(Clone)]
pub struct Name(Repr);

#[derive(Clone)]
enum Repr {
    Inline { len: u8, buf: [u8; INLINE] },
    Heap(Box<str>),
}

impl Name {
    pub fn new(s: &str) -> Name {
        if s.len() <= INLINE {
            let mut buf = [0u8; INLINE];
            buf[..s.len()].copy_from_slice(s.as_bytes());
            Name(Repr::Inline {
                len: s.len() as u8,
                buf,
            })
        } else {
            Name(Repr::Heap(s.into()))
        }
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        match &self.0 {
            // the bytes were copied from a `&str`
            Repr::Inline { len, buf } => unsafe {
                std::str::from_utf8_unchecked(&buf[..*len as usize])
            },
            Repr::Heap(b) => b,
        }
    }
}

impl Borrow<str> for Name {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq for Name {
    fn eq(&self, other: &Name) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for Name {}

impl Hash for Name {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

/// A map from names to values that keeps its entries in insertion order.
pub struct SmallMap<V> {
    entries: Vec<(Name, V)>,
    index: Option<FxHashMap<Name, usize>>,
}

impl<V> Default for SmallMap<V> {
    fn default() -> Self {
        SmallMap {
            entries: Vec::new(),
            index: None,
        }
    }
}

impl<V> SmallMap<V> {
    /// An empty map with room for `n` entries.
    pub fn with_capacity(n: usize) -> SmallMap<V> {
        SmallMap {
            entries: Vec::with_capacity(n),
            index: None,
        }
    }

    #[inline]
    pub fn position(&self, key: &str) -> Option<usize> {
        match &self.index {
            Some(ix) => ix.get(key).copied(),
            None => self.entries.iter().position(|(k, _)| k.as_str() == key),
        }
    }

    /// The value at `index`, if that entry is the one named `key` (a cached position).
    #[inline]
    pub fn get_at(&self, index: usize, key: &str) -> Option<&V> {
        match self.entries.get(index) {
            Some((k, v)) if k.as_str() == key => Some(v),
            _ => None,
        }
    }

    #[inline]
    pub fn get_mut_at(&mut self, index: usize, key: &str) -> Option<&mut V> {
        match self.entries.get_mut(index) {
            Some((k, v)) if k.as_str() == key => Some(v),
            _ => None,
        }
    }

    #[inline]
    pub fn get(&self, key: &str) -> Option<&V> {
        self.position(key).map(|i| &self.entries[i].1)
    }

    #[inline]
    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        self.position(key).map(|i| &mut self.entries[i].1)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.position(key).is_some()
    }

    /// Adds an entry whose name is known not to be in the map yet (a call's parameters, which
    /// the checker made distinct): no search for an existing one.
    #[inline]
    pub fn push_new(&mut self, key: &str, value: V) {
        debug_assert!(self.position(key).is_none());
        let name = Name::new(key);
        if let Some(ix) = &mut self.index {
            ix.insert(name.clone(), self.entries.len());
        }
        self.entries.push((name, value));
        if self.index.is_none() && self.entries.len() > INDEX_FROM {
            self.index = Some(
                self.entries
                    .iter()
                    .enumerate()
                    .map(|(i, (k, _))| (k.clone(), i))
                    .collect(),
            );
        }
    }

    /// Insert or replace.
    pub fn insert(&mut self, key: &str, value: V) {
        if let Some(i) = self.position(key) {
            self.entries[i].1 = value;
            return;
        }
        let name = Name::new(key);
        if let Some(ix) = &mut self.index {
            ix.insert(name.clone(), self.entries.len());
        }
        self.entries.push((name, value));
        if self.index.is_none() && self.entries.len() > INDEX_FROM {
            self.index = Some(
                self.entries
                    .iter()
                    .enumerate()
                    .map(|(i, (k, _))| (k.clone(), i))
                    .collect(),
            );
        }
    }

    /// The values, in order (the names stay, the map is not usable as before).
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.iter().map(|(_, v)| v)
    }

    /// Takes all the values out, leaving the map empty.
    pub fn take_values(&mut self) -> Vec<V> {
        self.index = None;
        std::mem::take(&mut self.entries)
            .into_iter()
            .map(|(_, v)| v)
            .collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }
}
