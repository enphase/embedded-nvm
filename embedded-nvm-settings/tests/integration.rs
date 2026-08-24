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

use embassy_futures::block_on;
use embedded_nvm_settings::{
    CommitError, LoadError, NvmSettings, PostcardFormat, SettingsFormat, StorageBackend,
};

// ---------------------------------------------------------------------------
// In-memory storage backend for testing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryBackendError {
    Injected,
}

/// A trivial in-memory [`StorageBackend`] for tests.
///
/// Stores at most one blob. Set `fail` to inject errors on the next operation.
pub struct MemoryBackend {
    data: Option<Vec<u8>>,
    fail: bool,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self {
            data: None,
            fail: false,
        }
    }

    pub fn with_data(data: &[u8]) -> Self {
        Self {
            data: Some(data.to_vec()),
            fail: false,
        }
    }
}

impl StorageBackend for MemoryBackend {
    type Error = MemoryBackendError;

    async fn load(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        if self.fail {
            return Err(MemoryBackendError::Injected);
        }
        match &self.data {
            Some(d) => {
                let n = d.len();
                buf[..n].copy_from_slice(d);
                Ok(Some(n))
            }
            None => Ok(None),
        }
    }

    async fn store(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        if self.fail {
            return Err(MemoryBackendError::Injected);
        }
        self.data = Some(data.to_vec());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Test settings type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct TestSettings {
    a: u32,
    b: u16,
}

type Format = PostcardFormat<64>;
type TestNvm = NvmSettings<TestSettings, MemoryBackend, Format, 64>;

fn fmt() -> Format {
    PostcardFormat
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn new_returns_default() {
    let nvm = TestNvm::new(MemoryBackend::new(), fmt());
    assert_eq!(nvm.get(), TestSettings::default());
}

#[test]
fn load_empty_returns_false() {
    block_on(async {
        let nvm = TestNvm::new(MemoryBackend::new(), fmt());
        assert_eq!(nvm.load().await.unwrap(), false);
        assert_eq!(nvm.get(), TestSettings::default());
    });
}

#[test]
fn load_preseeded_returns_true() {
    block_on(async {
        let value = TestSettings { a: 42, b: 7 };
        let mut buf = [0u8; 64];
        let n = fmt().serialize(&value, &mut buf).unwrap();

        let nvm = TestNvm::new(MemoryBackend::with_data(&buf[..n]), fmt());
        assert_eq!(nvm.load().await.unwrap(), true);
        assert_eq!(nvm.get(), value);
    });
}

#[test]
fn load_corrupt_returns_deserialize_error() {
    block_on(async {
        let nvm = TestNvm::new(MemoryBackend::with_data(&[0xFF; 8]), fmt());
        assert!(matches!(nvm.load().await, Err(LoadError::Deserialize(_))));
        assert_eq!(nvm.get(), TestSettings::default());
    });
}

#[test]
fn load_io_error_returns_storage_error() {
    block_on(async {
        let mut backend = MemoryBackend::new();
        backend.fail = true;
        let nvm = TestNvm::new(backend, fmt());
        assert!(matches!(
            nvm.load().await,
            Err(LoadError::Storage(MemoryBackendError::Injected))
        ));
    });
}

#[test]
fn update_changes_value() {
    let nvm = TestNvm::new(MemoryBackend::new(), fmt());
    nvm.update(|s| TestSettings { a: 99, ..s });
    assert_eq!(nvm.get().a, 99);
}

#[test]
fn commit_persists_and_roundtrips() {
    block_on(async {
        let nvm = TestNvm::new(MemoryBackend::new(), fmt());
        let expected = TestSettings { a: 123, b: 456 };
        nvm.update(|_| expected);
        nvm.commit().await.unwrap();
        assert_eq!(nvm.get(), expected);
    });
}

#[test]
fn commit_store_error_restores_dirty() {
    block_on(async {
        let mut backend = MemoryBackend::new();
        backend.fail = true;
        let nvm = TestNvm::new(backend, fmt());
        nvm.update(|_| TestSettings { a: 1, b: 2 });

        let result = nvm.commit().await;
        assert!(matches!(
            result,
            Err(CommitError::Storage(MemoryBackendError::Injected))
        ));
        assert_eq!(nvm.get(), TestSettings { a: 1, b: 2 });
    });
}

#[test]
fn multiple_updates_coalesce() {
    block_on(async {
        let nvm = TestNvm::new(MemoryBackend::new(), fmt());
        nvm.update(|_| TestSettings { a: 1, b: 0 });
        nvm.update(|s| TestSettings { a: s.a + 1, ..s });
        nvm.update(|s| TestSettings { a: s.a + 1, ..s });
        nvm.commit().await.unwrap();
        assert_eq!(nvm.get().a, 3);
    });
}

// ---------------------------------------------------------------------------
// SettingsFormat trait tests
// ---------------------------------------------------------------------------

#[test]
fn plain_postcard_roundtrip() {
    let value = TestSettings {
        a: 0xDEAD,
        b: 0xBEEF,
    };
    let mut buf = [0u8; 64];
    let n = fmt().serialize(&value, &mut buf).unwrap();
    let decoded = fmt().deserialize(&buf[..n]);
    assert_eq!(decoded, Ok(value));
}

#[test]
fn plain_postcard_corrupt_returns_err() {
    let decoded: Result<TestSettings, _> = fmt().deserialize(&[0xFF; 64]);
    assert!(decoded.is_err());
}
