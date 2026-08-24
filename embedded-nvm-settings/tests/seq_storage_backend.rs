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

/// Integration tests for SeqStorageBackend against a real sequential-storage MockFlash.
///
/// These tests exercise the full load/store path through SeqStorageBackend.
use embassy_futures::block_on;
use embedded_nvm_settings::{NvmSettings, PostcardFormat, SeqStorageBackend};
use embedded_storage_async::nor_flash::{ErrorType, MultiwriteNorFlash, NorFlash, ReadNorFlash};
use sequential_storage::cache::NoCache;
use sequential_storage::mock_flash::{MockFlashBase, MockFlashError, WriteCountCheck};
use std::sync::{Arc, Mutex};

// 4 pages × 64 words × 4 bytes/word = 1024 bytes total flash.
type MockFlash = MockFlashBase<4, 4, 64>;

const FLASH_RANGE_END: u32 = MockFlash::FULL_FLASH_RANGE.end;
const BUF_SIZE: usize = 64;

type SeqBackend = SeqStorageBackend<SharedFlash, NoCache, 0, FLASH_RANGE_END, BUF_SIZE>;
type SeqNvm = NvmSettings<TestSettings, SeqBackend, PostcardFormat<BUF_SIZE>, BUF_SIZE>;

// Allows flash to be shared across multiple settings, to simulate power cycling.
#[derive(Clone)]
struct SharedFlash(Arc<Mutex<MockFlash>>);

impl SharedFlash {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(MockFlash::new(
            WriteCountCheck::Twice, // Twice: allows sequential-storage's double-write erase pattern
            None,
            true,
        ))))
    }
}

impl ErrorType for SharedFlash {
    type Error = MockFlashError;
}

impl ReadNorFlash for SharedFlash {
    const READ_SIZE: usize = MockFlash::READ_SIZE;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        self.0.lock().unwrap().read(offset, bytes).await
    }

    fn capacity(&self) -> usize {
        FLASH_RANGE_END as usize
    }
}

impl MultiwriteNorFlash for SharedFlash {}

impl NorFlash for SharedFlash {
    const WRITE_SIZE: usize = MockFlash::WRITE_SIZE;
    const ERASE_SIZE: usize = MockFlash::ERASE_SIZE;

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        self.0.lock().unwrap().write(offset, bytes).await
    }

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        self.0.lock().unwrap().erase(from, to).await
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct TestSettings {
    a: u32,
    b: u16,
}

fn make_nvm(flash: SharedFlash) -> SeqNvm {
    SeqNvm::new(
        SeqStorageBackend::new(flash, NoCache::new()),
        PostcardFormat,
    )
}

#[test]
fn seq_backend_store_and_load_roundtrip() {
    block_on(async {
        let flash = SharedFlash::new();
        let expected = TestSettings {
            a: 0xDEAD_BEEF,
            b: 0x1234,
        };

        // --- Cycle 1: write ---
        {
            let nvm = make_nvm(flash.clone());
            nvm.update(|_| expected);
            nvm.commit().await.expect("commit failed");
        }

        // --- Cycle 2: power cycle (same flash, new NvmSettings) ---
        {
            let nvm = make_nvm(flash.clone());
            let found = nvm.load().await.expect("load failed");
            assert!(found, "expected a stored record");
            assert_eq!(nvm.get(), expected);
        }
    });
}

#[test]
fn seq_backend_blank_flash_returns_default() {
    block_on(async {
        let flash = SharedFlash::new();
        let nvm = make_nvm(flash);
        let found = nvm
            .load()
            .await
            .expect("load should not error on blank flash");
        assert!(!found);
        assert_eq!(nvm.get(), TestSettings::default());
    });
}
