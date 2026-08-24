// Copyright 2026 Enphase Energy, Inc.
//
//    Licensed under the Apache License, Version 2.0 (the "License");
//    you may not use this file except in compliance with the License.
//    You may obtain a copy of the License at
//
//        http://www.apache.org/licenses/LICENSE-2.0
//
//    Unless required by applicable law or agreed to in writing, software
//    distributed under the License is distributed on an "AS IS" BASIS,
//    WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//    See the License for the specific language governing permissions and
//    limitations under the License.

//! Cached persistent settings for embedded systems with deferred async commit.
#![cfg_attr(not(test), no_std)]

// Test README examples as doctests.
#[cfg(doctest)]
#[doc = include_str!("../../README.md")]
struct ReadmeDoctests;

mod settings_format;
mod storage_backend;

#[cfg(feature = "sequential-storage")]
mod backend_seq_storage;
#[cfg(feature = "postcard")]
mod format_postcard;
#[cfg(feature = "versioned-postcard")]
mod format_versioned_postcard;

pub use settings_format::SettingsFormat;
pub use storage_backend::StorageBackend;

#[cfg(feature = "sequential-storage")]
pub use backend_seq_storage::SeqStorageBackend;
#[cfg(feature = "postcard")]
pub use format_postcard::PostcardFormat;
#[cfg(feature = "versioned-postcard")]
pub use format_versioned_postcard::VersionedPostcardFormat;

/// Convenience alias for the common configuration: sequential-storage backend
/// with versioned-postcard format.
///
/// Set `BUF_SIZE` to `<T as versioned_postcard::Version>::RECORD_MAX + 1`.
/// Automatic derivation is not yet supported on stable Rust because of `generic_const_exprs`.
#[cfg(all(feature = "sequential-storage", feature = "versioned-postcard"))]
pub type SeqNvmSettings<T, S, C, const NVM_START: u32, const NVM_SIZE: u32, const BUF_SIZE: usize> =
    NvmSettings<
        T,
        SeqStorageBackend<S, C, NVM_START, NVM_SIZE, BUF_SIZE>,
        VersionedPostcardFormat<BUF_SIZE>,
        BUF_SIZE,
    >;

use core::cell::Cell;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;

/// Error returned by [`NvmSettings::load`].
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LoadError<FE: core::fmt::Debug, BE: core::fmt::Debug> {
    Deserialize(FE),
    Storage(BE),
}

/// Error returned by [`NvmSettings::commit`].
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum CommitError<SE: core::fmt::Debug, BE: core::fmt::Debug> {
    Serialize(SE),
    Storage(BE),
}

/// In-memory value plus a flag tracking whether it has diverged from storage.
#[derive(Clone, Copy)]
struct Cached<T> {
    value: T,
    dirty: bool,
}

/// Cached persistent settings with deferred async commit.
///
/// Reads ([`get`](Self::get)) and writes ([`update`](Self::update)) hit an
/// in-memory cache and are **immediate, synchronous, and atomic** (a
/// `CriticalSectionRawMutex`-guarded `Cell`), safe to call straight from a
/// sync closure or ISR. A write only marks the cache dirty, the blocking
/// storage write is deferred to [`commit`](Self::commit), which is async and a
/// no-op unless the cache is dirty. The caller decides when to `commit`,
/// keeping flash off the hot path and allowing many edits to coalesce.
///
/// # Concurrency & ISR safety
///
/// [`get`](Self::get)/[`update`](Self::update) only take a
/// `CriticalSectionRawMutex` critical section and should never wait and are
/// safe from any context including ISRs.
///
/// [`commit`](Self::commit) is `async` and takes an async `Mutex`, so it can
/// only run inside a task (never a raw ISR) and yields rather than spins when
/// contended — also deadlock-free under cooperative scheduling. Never `block_on`
/// it; from an ISR or critical section that can hang. The cache lock is held
/// only briefly and never across the commit `.await`, so there is no lock-order
/// cycle. To persist from a sync/ISR context, signal an async owner to call
/// `commit`.
///
/// # Type parameters
///
/// - `T`: The settings value type.
/// - `B`: Storage backend, implementing a crate-defined trait.
/// - `F`: Serialization format, implementing a crate-defined trait.
/// - `BUF_SIZE`: Buffer size for format's serialized output.
///   Automatic derivation is not yet supported on stable Rust because of `generic_const_exprs`.
pub struct NvmSettings<
    T: Clone + Copy + Default + PartialEq,
    B: StorageBackend,
    F: SettingsFormat<T>,
    const BUF_SIZE: usize,
> {
    cache: BlockingMutex<CriticalSectionRawMutex, Cell<Cached<T>>>,
    writer: Mutex<CriticalSectionRawMutex, B>,
    format: F,
}

impl<
    T: Clone + Copy + Default + PartialEq,
    B: StorageBackend,
    F: SettingsFormat<T>,
    const BUF_SIZE: usize,
> NvmSettings<T, B, F, BUF_SIZE>
{
    /// Create a new `NvmSettings` initialised to `T::default()`.
    ///
    /// This is synchronous and infallible. Call [`load`](Self::load) afterwards
    /// to populate the cache from storage.
    pub fn new(backend: B, format: F) -> Self {
        Self {
            cache: BlockingMutex::new(Cell::new(Cached {
                value: T::default(),
                dirty: false,
            })),
            writer: Mutex::new(backend),
            format,
        }
    }

    /// Load settings from storage into the cache.
    ///
    /// Returns:
    /// - `Ok(true)`:a stored record was loaded successfully.
    /// - `Ok(false)`: no stored record found, the cache retains its default.
    /// - `Err(...)`: propagates underlying error from format or storage backend
    pub async fn load(&mut self) -> Result<bool, LoadError<F::Error, B::Error>> {
        let mut backend = self.writer.lock().await;
        let mut buf = [0u8; BUF_SIZE];
        match backend.load(&mut buf).await {
            Ok(Some(n)) => match self.format.deserialize(&buf[..n]) {
                Ok(value) => {
                    self.cache.lock(|c| {
                        c.set(Cached {
                            value,
                            dirty: false,
                        })
                    });
                    Ok(true)
                }
                Err(e) => Err(LoadError::Deserialize(e)),
            },
            Ok(None) => Ok(false),
            Err(e) => Err(LoadError::Storage(e)),
        }
    }

    /// Read the current cached value. Wait-free and ISR-safe.
    pub fn get(&self) -> T {
        self.cache.lock(|c| c.get().value)
    }

    /// Apply `f` to the cached value, updating the cached value.
    /// Synchronous, atomic, wait-free and ISR-safe.
    /// `f` runs inside a critical section (interrupts disabled), so keep it short.
    ///
    /// Does not write flash, only marks the cache dirty if there is a change.
    /// Persist later via [`commit`](Self::commit).
    pub fn update(&self, f: impl FnOnce(T) -> T) {
        self.cache.lock(|c| {
            let cur = c.get();
            let new = f(cur.value);
            c.set(Cached {
                value: new,
                dirty: cur.dirty || new != cur.value,
            });
        });
    }

    /// Persist the cached value to storage if the cache is marked dirty.
    /// No-op when cache is clean.
    ///
    /// On error the dirty flag is restored so a later commit retries.
    pub async fn commit(&self) -> Result<(), CommitError<F::Error, B::Error>> {
        // Atomically snapshot the value and clear dirty before the async write.
        // If a concurrent update happens, the newer value is picked up by the next commit.
        let value = match self.cache.lock(|c| {
            let cur = c.get();
            if cur.dirty {
                c.set(Cached {
                    value: cur.value,
                    dirty: false,
                });
                Some(cur.value)
            } else {
                None
            }
        }) {
            Some(v) => v,
            None => return Ok(()),
        };

        let mut rec = [0u8; BUF_SIZE];
        let len = match self.format.serialize(&value, &mut rec) {
            Ok(n) => n,
            Err(e) => {
                self.cache.lock(|c| {
                    c.set(Cached {
                        value: c.get().value,
                        dirty: true,
                    })
                });
                return Err(CommitError::Serialize(e));
            }
        };

        let mut backend = self.writer.lock().await;
        match backend.store(&rec[..len]).await {
            Ok(()) => Ok(()),
            Err(e) => {
                self.cache.lock(|c| {
                    c.set(Cached {
                        value: c.get().value,
                        dirty: true,
                    })
                });
                Err(CommitError::Storage(e))
            }
        }
    }
}
