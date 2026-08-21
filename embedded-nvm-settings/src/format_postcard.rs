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

/// [`SettingsFormat`] using plain [`postcard`].
///
/// `BUF_SIZE` must be specified by the caller since postcard has no
/// compile-time max-size mechanism.
///
/// Zero-sized — carries no runtime state.
pub struct PostcardFormat<const BUF_SIZE: usize>;

impl<T, const BUF_SIZE: usize> SettingsFormat<T> for PostcardFormat<BUF_SIZE>
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de>,
{
    type Error = postcard::Error;

    fn serialize(&self, value: &T, buf: &mut [u8]) -> Result<usize, Self::Error> {
        Ok(postcard::to_slice(value, buf)?.len())
    }

    fn deserialize(&self, record: &[u8]) -> Result<T, Self::Error> {
        postcard::from_bytes(record)
    }
}
