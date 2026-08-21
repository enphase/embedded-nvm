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
