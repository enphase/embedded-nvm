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

use crate::SettingsFormat;
use versioned_postcard::Version;

/// [`SettingsFormat`] using [`versioned_postcard`] for versioned, migratable
/// serialization.
///
/// Set `BUF_SIZE` to `T::RECORD_MAX + 1`. Automatic derivation is not yet supported
/// on stable Rust because of generic_const_exprs.
///
/// Zero-sized — carries no runtime state.
pub struct PostcardVersionedFormat<const BUF_SIZE: usize>;

impl<T, const BUF_SIZE: usize> SettingsFormat<T> for PostcardVersionedFormat<BUF_SIZE>
where
    T: Copy + Version,
    T::Wire: serde::Serialize + From<T>,
{
    type Error = versioned_postcard::Error;

    fn serialize(&self, value: &T, buf: &mut [u8]) -> Result<usize, Self::Error> {
        value.serialize_into(buf)
    }

    fn deserialize(&self, record: &[u8]) -> Result<T, Self::Error> {
        T::deserialize_from::<BUF_SIZE>(record)
    }
}
