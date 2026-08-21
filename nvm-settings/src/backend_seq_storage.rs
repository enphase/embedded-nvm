use embedded_storage_async::nor_flash::NorFlash;
use sequential_storage::cache::KeyCacheImpl;
use sequential_storage::map::{MapConfig, MapStorage};

use crate::StorageBackend;

const SETTINGS_KEY: u8 = 0;

/// [`StorageBackend`] backed by [`sequential_storage::map::MapStorage`].
///
/// Provides wear-leveled key-value storage on NOR flash.
///
/// `BUF_SIZE` is the working-buffer size for `sequential-storage` operations.
pub struct SeqStorageBackend<
    S: NorFlash,
    C: KeyCacheImpl<u8>,
    const NVM_START: u32,
    const NVM_SIZE: u32,
    const BUF_SIZE: usize,
> {
    storage: MapStorage<u8, S, C>,
}

impl<
    S: NorFlash,
    C: KeyCacheImpl<u8>,
    const NVM_START: u32,
    const NVM_SIZE: u32,
    const BUF_SIZE: usize,
> SeqStorageBackend<S, C, NVM_START, NVM_SIZE, BUF_SIZE>
{
    /// Construct from a pre-built [`MapStorage`]. Use this when you need to
    /// configure the storage (range, cache type) yourself.
    pub fn from_storage(storage: MapStorage<u8, S, C>) -> Self {
        Self { storage }
    }

    /// Convenience constructor: build [`MapStorage`] from raw flash and cache,
    /// using `NVM_START` and `NVM_SIZE` as the flash range.
    pub fn new(flash: S, cache: C) -> Self {
        Self::from_storage(MapStorage::new(
            flash,
            const { MapConfig::new(NVM_START..NVM_START + NVM_SIZE) },
            cache,
        ))
    }
}

impl<
    S: NorFlash,
    C: KeyCacheImpl<u8>,
    const NVM_START: u32,
    const NVM_SIZE: u32,
    const BUF_SIZE: usize,
> StorageBackend for SeqStorageBackend<S, C, NVM_START, NVM_SIZE, BUF_SIZE>
{
    type Error = sequential_storage::Error<S::Error>;

    async fn load(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        // Use a local scratch buffer for fetch_item so we can copy the returned
        // record slice into the caller's buf without borrow-checker aliasing issues.
        // (fetch_item returns a &[u8] subslice of the scratch buffer after the key bytes.)
        let mut scratch = [0u8; BUF_SIZE];
        match self.storage.fetch_item::<&[u8]>(&mut scratch, &SETTINGS_KEY).await {
            Ok(Some(record)) => {
                let n = record.len();
                buf[..n].copy_from_slice(record);
                Ok(Some(n))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn store(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        let mut work_buf = [0u8; BUF_SIZE];
        self.storage.store_item(&mut work_buf, &SETTINGS_KEY, &data).await
    }
}
