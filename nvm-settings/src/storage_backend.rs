/// Abstraction for async single-slot byte-blob storage, used during loads and commits.
#[allow(async_fn_in_trait)]
pub trait StorageBackend {
    type Error: core::fmt::Debug;

    /// Load the stored blob into `buf`.
    ///
    /// Returns `Ok(Some(n))` where the record occupies `buf[..n]`,
    /// `Ok(None)` if no record exists, or `Err` on I/O failure.
    async fn load(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Self::Error>;

    /// Store `data` persistently, replacing any previous value.
    async fn store(&mut self, data: &[u8]) -> Result<(), Self::Error>;
}
