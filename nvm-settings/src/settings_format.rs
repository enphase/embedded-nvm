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

/// Serialize and deserialize a settings value of type `T` to/from a byte buffer.
///
/// Implementations are generally stateless, but `&self` is provided for extensibility.
pub trait SettingsFormat<T> {
    type Error: core::fmt::Debug;

    /// Serialize `value` into `buf`, returning the number of bytes written.
    fn serialize(&self, value: &T, buf: &mut [u8]) -> Result<usize, Self::Error>;

    /// Deserialize a value from `record`.
    fn deserialize(&self, record: &[u8]) -> Result<T, Self::Error>;
}
