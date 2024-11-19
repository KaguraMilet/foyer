//  Copyright 2024 foyer Project Authors
//
//  Licensed under the Apache License, Version 2.0 (the "License");
//  you may not use this file except in compliance with the License.
//  You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.

//! Test utils for the `foyer-storage` crate.

use std::{
    borrow::Borrow,
    collections::HashSet,
    fmt::Debug,
    hash::{BuildHasher, Hash, Hasher},
    sync::{Arc, OnceLock},
};

use foyer_common::{code::StorageKey, metrics::Metrics};
use parking_lot::Mutex;

use crate::{
    picker::{AdmissionPicker, ReinsertionPicker},
    statistics::Statistics,
};

/// A phantom metrics for test.
static METRICS_FOR_TEST: OnceLock<Metrics> = OnceLock::new();

/// Get a phantom metrics for test.
pub fn metrics_for_test() -> &'static Metrics {
    METRICS_FOR_TEST.get_or_init(|| Metrics::new("test"))
}

/// A picker that only admits key from the given list.
pub struct BiasedPicker<K> {
    admits: HashSet<K>,
}

impl<K> Debug for BiasedPicker<K>
where
    K: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BiasedPicker").field("admits", &self.admits).finish()
    }
}

impl<K> BiasedPicker<K> {
    /// Create a biased picker with the given admit list.
    pub fn new(admits: impl IntoIterator<Item = K>) -> Self
    where
        K: Hash + Eq,
    {
        Self {
            admits: admits.into_iter().collect(),
        }
    }
}

impl<K> AdmissionPicker for BiasedPicker<K>
where
    K: Send + Sync + 'static + Hash + Eq + Debug,
{
    type Key = K;

    fn pick(&self, _: &Arc<Statistics>, key: &Self::Key) -> bool {
        self.admits.contains(key)
    }
}

impl<K> ReinsertionPicker for BiasedPicker<K>
where
    K: Send + Sync + 'static + Hash + Eq + Debug,
{
    type Key = K;

    fn pick(&self, _: &Arc<Statistics>, key: &Self::Key) -> bool {
        self.admits.contains(key.borrow())
    }
}

/// The record entry for admission and eviction.
#[derive(Debug, Clone)]
pub enum Record<K> {
    /// Admission record entry.
    Admit(K),
    /// Eviction record entry.
    Evict(K),
}

/// A recorder that records the cache entry admission and eviction of a disk cache.
///
/// [`Recorder`] should be used as both the admission picker and the reinsertion picker to record.
pub struct Recorder<K> {
    records: Mutex<Vec<Record<K>>>,
}

impl<K> Debug for Recorder<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JudgeRecorder").finish()
    }
}

impl<K> Recorder<K>
where
    K: StorageKey + Clone,
{
    /// Dump the record entries of the recorder.
    pub fn dump(&self) -> Vec<Record<K>> {
        self.records.lock().clone()
    }

    /// Get the hash set of the remaining key at the moment.
    pub fn remains(&self) -> HashSet<K> {
        let records = self.dump();
        let mut res = HashSet::default();
        for record in records {
            match record {
                Record::Admit(key) => {
                    res.insert(key);
                }
                Record::Evict(key) => {
                    res.remove(&key);
                }
            }
        }
        res
    }
}

impl<K> Default for Recorder<K>
where
    K: StorageKey,
{
    fn default() -> Self {
        Self {
            records: Mutex::new(Vec::default()),
        }
    }
}

impl<K> AdmissionPicker for Recorder<K>
where
    K: StorageKey + Clone,
{
    type Key = K;

    fn pick(&self, _: &Arc<Statistics>, key: &Self::Key) -> bool {
        self.records.lock().push(Record::Admit(key.clone()));
        true
    }
}

impl<K> ReinsertionPicker for Recorder<K>
where
    K: StorageKey + Clone,
{
    type Key = K;

    fn pick(&self, _: &Arc<Statistics>, key: &Self::Key) -> bool {
        self.records.lock().push(Record::Evict(key.clone()));
        false
    }
}

/// A hasher return u64 mod result.
#[derive(Debug, Default)]
pub struct ModHasher {
    state: u64,
}

impl Hasher for ModHasher {
    fn finish(&self) -> u64 {
        self.state
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state = (self.state << 8) + *byte as u64;
        }
    }
}

impl BuildHasher for ModHasher {
    type Hasher = Self;

    fn build_hasher(&self) -> Self::Hasher {
        Self::default()
    }
}
