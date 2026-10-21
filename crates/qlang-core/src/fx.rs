//! A fast, non-cryptographic hasher (the "Fx" hash of rustc) for the interpreter's own tables:
//! variable names and node ids, which come from the program text and never from data an
//! attacker could choose to cause collisions. Maps keyed by values the program computes
//! (qlang's own `map`) keep the standard hasher.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

#[derive(Default, Clone, Copy)]
pub struct FxHasher {
    hash: u64,
}

impl FxHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        while rest.len() >= 8 {
            let (word, tail) = rest.split_at(8);
            self.add(u64::from_le_bytes(word.try_into().expect("8 bytes")));
            rest = tail;
        }
        if !rest.is_empty() {
            let mut buf = [0u8; 8];
            buf[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(buf) ^ ((rest.len() as u64) << 56));
        }
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add(i as u64);
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add(i as u64);
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

pub type FxHashMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;

pub type FxHashSet<K> = std::collections::HashSet<K, BuildHasherDefault<FxHasher>>;

/// A map from node ids to values, stored by index: node ids are small consecutive numbers,
/// so a lookup is one array access (the interpreter does one per name it evaluates).
pub struct NodeMap<T> {
    slots: Vec<Option<T>>,
}

impl<T> Default for NodeMap<T> {
    fn default() -> Self {
        NodeMap { slots: Vec::new() }
    }
}

impl<T> NodeMap<T> {
    #[inline]
    pub fn get(&self, id: &u32) -> Option<&T> {
        self.slots.get(*id as usize).and_then(Option::as_ref)
    }

    pub fn insert(&mut self, id: u32, value: T) -> Option<T> {
        let i = id as usize;
        if i >= self.slots.len() {
            self.slots.resize_with(i + 1, || None);
        }
        self.slots[i].replace(value)
    }

    pub fn contains_key(&self, id: &u32) -> bool {
        self.get(id).is_some()
    }
}
