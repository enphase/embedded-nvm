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

//! Versioned, migratable [postcard] records for `no_std`, storage-agnostic.
//! Intended for embedded applications; uses minimal flash in terms of storage and program size.
//!
//! Each schema (serializable type) implements the [`Version`] trait, which defines a version tag,
//! **[`Prev`](Version::Prev)** schema, and migration code (if any).
//!
//! Define `Prev` as [`Base`] for the oldest version.
//!
//! Records are framed `[ varint tag ][ postcard(wire payload) ]`. Loading works its way down the
//! `Prev` chain, upgrading each version from its `Prev`, until it reaches the latest version.
//! Compile-time asserts require version tags to be strictly increasing, which ensures loading
//! terminates.
//!
//! Versions may also optionally define a separate **[`Wire`](Version::Wire)** struct, allowing for a
//! different on-flash representation. If used, it must implement `From<Wire>` (normalization) and
//! `From<Self>` (denormalization). This provides a lightweight way to support append-only fields
//! without a new tag version: an appended field is an `Option` in the wire format that decodes to
//! `None` from zero-padded input, and normalization supplies a default; denormalization converts it
//! back to `Some(..)`.
//!
//! The [`Wire`](Version::Wire) struct must implement [`WireSize`], its max serialized size, either:
//! - manually, by implementing `WireSize` and defining `SIZE`, or
//! - automatically, with the `max-size` feature and `#[derive(MaxSize)]` (see [`max_size`]).
//!
//! [`RECORD_MAX`](Version::RECORD_MAX) is the max serialized record size (payload + tag) across all
//! versions down the `Prev` chain — size your buffers to it.
//!
//! ```
//! use postcard_versioned::{Base, Version, WireSize};
//!
//! // Normalized, app-facing type — no `Option`, no serde.
//! #[derive(Clone, Copy, PartialEq, Debug)]
//! struct Cfg { a: u16, b: i16 }
//! impl Default for Cfg { fn default() -> Self { Cfg { a: 0, b: 99 } } }
//!
//! // On-flash wire form — append-only `b` is `Option`.
//! #[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
//! struct CfgWire { a: u16, b: Option<i16> }
//! impl WireSize for CfgWire {
//!     // Postcard max: a (u16, <=3) + b (Option<i16>: 1 tag + <=3). Compute it however you like,
//!     // or use the `max-size` feature to derive it.
//!     const SIZE: usize = 3 + (1 + 3);
//! }
//! impl From<CfgWire> for Cfg { fn from(w: CfgWire) -> Cfg { Cfg { a: w.a, b: w.b.unwrap_or(Cfg::default().b) } } }
//! impl From<Cfg> for CfgWire { fn from(c: Cfg) -> CfgWire { CfgWire { a: c.a, b: Some(c.b) } } }
//!
//! impl Version for Cfg {
//!     const TAG: u16 = 1;
//!     type Prev = Base;
//!     type Wire = CfgWire;
//!     fn from_prev(p: Base) -> Cfg { match p {} }
//! }
//!
//! let mut buf = [0u8; 1 + Cfg::RECORD_MAX]; // zero-initialized; record goes in the prefix
//! let n = Cfg { a: 7, b: -3 }.serialize_into(&mut buf).unwrap();
//! assert_eq!(Cfg::deserialize_from::<{ 1 + Cfg::RECORD_MAX }>(&buf[..n]).unwrap(), Cfg { a: 7, b: -3 });
//! ```

#![cfg_attr(not(test), no_std)]

mod sizing;

#[cfg(feature = "max-size")]
pub mod max_size;

use sizing::varint_max_bytes;

/// Backend-agnostic (de)serialization error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// The output buffer was too small to hold the serialized record.
    BufferTooSmall,
    /// The record could not be decoded (bad bytes, unknown tag, short buffer).
    Corrupt,
}

/// Maximum postcard-serialized size of a wire payload (excludes the record tag). Named by
/// [`Version::Wire`] and folded into [`Version::RECORD_MAX`].
///
/// Implement it by hand (compute the bound yourself), or automatically with the `max-size` feature
/// and `#[derive(MaxSize)]` on the wire type (see [`max_size`]).
pub trait WireSize {
    /// Max serialized size of this wire type's payload.
    const SIZE: usize;
}

const fn cmax(a: usize, b: usize) -> usize {
    if a > b { a } else { b }
}

/// Write `[ varint tag ][ postcard(value) ]` into `buf`, returning the bytes used. Internal framing
/// used by [`Version::serialize_into`].
pub(crate) fn write_versioned<T: serde::Serialize>(
    tag: u16,
    value: &T,
    buf: &mut [u8],
) -> Result<usize, Error> {
    let n1 = postcard::to_slice(&tag, buf)
        .map_err(|_| Error::BufferTooSmall)?
        .len();
    let n2 = postcard::to_slice(value, buf.get_mut(n1..).ok_or(Error::BufferTooSmall)?)
        .map_err(|_| Error::BufferTooSmall)?
        .len();
    Ok(n1 + n2)
}

/// Recursion for [`Version::deserialize_from`]: decode `V`'s wire at the matching tag (then
/// normalize), else migrate one step up from `V::Prev`. `body` must have enough trailing zeros for
/// the largest wire (the caller zero-pads). Terminates at [`Base`] (tag `0`).
fn load<V: Version>(tag: u16, body: &[u8]) -> Result<V, Error> {
    if V::TAG == 0 {
        return Err(Error::Corrupt); // reached `Base` without a tag match
    }
    let _ = V::_TAGS_INCREASE; // force the compile-time ordering check for this version
    if tag == V::TAG {
        // `body` is the wire payload followed by zero padding; `from_bytes` reads this wire's fields
        // (an appended-absent `Option` reads its `0` tag byte as `None`) and ignores the rest.
        // `.into()` normalizes (`None -> default`).
        let wire: V::Wire = postcard::from_bytes(body).map_err(|_| Error::Corrupt)?;
        Ok(wire.into())
    } else {
        Ok(V::from_prev(load::<V::Prev>(tag, body)?))
    }
}

/// Uninhabited terminal of the version chain. The oldest schema sets `type Prev = Base`. Reserves
/// tag `0`; real versions use tags `>= 1`.
pub enum Base {}

// `Base` is its own `Wire`, so it must be `Deserialize` (never actually invoked — `load` stops at
// tag `0` before decoding). Under `max-size` the blanket in [`max_size`] supplies its `WireSize`
// via `MaxSize`; otherwise it is given here.
impl<'de> serde::Deserialize<'de> for Base {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Err(<D::Error as serde::de::Error>::custom("Base is uninhabited"))
    }
}
#[cfg(not(feature = "max-size"))]
impl WireSize for Base {
    const SIZE: usize = 0;
}
#[cfg(feature = "max-size")]
impl postcard::experimental::max_size::MaxSize for Base {
    const POSTCARD_MAX_SIZE: usize = 0;
}

/// A schema version: its tag, previous version, on-flash wire form, migration from the prior, and
/// (via `From`) how the wire normalizes into this app-facing type.
///
/// A version declaration is just: the normalized struct + `const TAG` + `type Prev` + `type Wire` +
/// `from_prev`, plus the `From<Wire>`/`From<Self>` pair (free when `Wire = Self`). Everything else —
/// varint framing, dispatch, the migration fold, compile-time sizing, and the public
/// [`serialize_into`](Version::serialize_into) / [`deserialize_from`](Version::deserialize_from) — is
/// provided here.
pub trait Version: Sized
where
    Self: From<Self::Wire>, // normalize (reflexive when `Wire = Self`); implied for the `Prev` spine
{
    /// On-flash tag (varint-encoded). Must be `> Prev::TAG`.
    const TAG: u16;

    /// The immediately older version, or [`Base`] for the oldest.
    type Prev: Version;

    /// Optional different on-flash form (e.g. with `Option<T>` append-only fields); use `= Self`
    /// otherwise.
    type Wire: for<'de> serde::Deserialize<'de> + WireSize;

    /// Build this version from its predecessor (the migration step). Never called when `Prev` is
    /// [`Base`]; convention is `match prev {}` (or `panic!`).
    fn from_prev(prev: Self::Prev) -> Self;

    /// Max serialized **record** size (largest wire payload across the whole `Prev` chain, plus the
    /// varint tag). A max, since versions may *shrink* — an old record can be larger than the latest.
    /// Size buffers to this, e.g. `[0u8; 1 + T::RECORD_MAX]` (zero-initialized). `Base` is `0`.
    const RECORD_MAX: usize = cmax(
        <Self::Wire as WireSize>::SIZE + varint_max_bytes(core::mem::size_of::<u16>()),
        <Self::Prev as Version>::RECORD_MAX,
    );

    /// Compile-time proof that tags strictly increase from oldest to latest (evaluated by `load`),
    /// which makes the `Prev` spine loop-free and guarantees it terminates at [`Base`].
    const _TAGS_INCREASE: () = assert!(
        Self::TAG > <Self::Prev as Version>::TAG,
        "version tags must strictly increase from oldest to latest",
    );

    /// Decode a `record` (a `[ varint tag ][ wire payload ]`, **not** required to be zero-padded)
    /// into this (latest) normalized version, migrating any older record up the `Prev` spine.
    ///
    /// Zero-padding is done here: append-only fields defined as `Option<T>` decode to `None` and are
    /// normalized to a default. `N` is the scratch/pad size and should be `1 + RECORD_MAX`; a record
    /// longer than `N` returns [`Error::Corrupt`].
    fn deserialize_from<const N: usize>(record: &[u8]) -> Result<Self, Error> {
        let mut scratch = [0u8; N];
        scratch
            .get_mut(..record.len())
            .ok_or(Error::Corrupt)?
            .copy_from_slice(record);
        let (tag, body) =
            postcard::take_from_bytes::<u16>(&scratch).map_err(|_| Error::Corrupt)?;
        load::<Self>(tag, body)
    }

    /// Serialize this normalized value as a tagged record (via its wire form), returning the length.
    /// Store `buf[..len]`; [`deserialize_from`](Version::deserialize_from) zero-pads on the way back
    /// in, so the stored bytes need no trailing padding.
    fn serialize_into(&self, buf: &mut [u8]) -> Result<usize, Error>
    where
        Self: Copy,
        Self::Wire: serde::Serialize + From<Self>,
    {
        write_versioned(Self::TAG, &Self::Wire::from(*self), buf)
    }
}

/// Terminal impl (written once): `from_prev` is total via the empty match; the fold/assert bottom
/// out; `load` stops at tag `0`.
impl Version for Base {
    const TAG: u16 = 0;
    type Prev = Base;
    type Wire = Base;
    const RECORD_MAX: usize = 0;
    const _TAGS_INCREASE: () = ();
    fn from_prev(prev: Base) -> Base {
        match prev {}
    }
}

#[cfg(test)]
mod tests {
    use super::sizing::varint_max_bytes;
    use super::*;

    // --- "growing" chain: `b` is an append-only `Option` in the wire; `None` -> default 99 ---
    #[derive(Clone, Copy, PartialEq, Debug)]
    struct Demo {
        a: u16,
        b: i16,
    }
    impl Default for Demo {
        fn default() -> Self {
            Demo { a: 0, b: 99 }
        }
    }
    #[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
    struct DemoWire {
        a: u16,
        b: Option<i16>,
    }
    impl WireSize for DemoWire {
        const SIZE: usize = varint_max_bytes(2) + (1 + varint_max_bytes(2)); // a + Option<i16>
    }
    impl From<DemoWire> for Demo {
        fn from(w: DemoWire) -> Demo {
            Demo { a: w.a, b: w.b.unwrap_or(Demo::default().b) }
        }
    }
    impl From<Demo> for DemoWire {
        fn from(d: Demo) -> DemoWire {
            DemoWire { a: d.a, b: Some(d.b) }
        }
    }
    impl Version for Demo {
        const TAG: u16 = 2;
        type Prev = DemoV0;
        type Wire = DemoWire;
        fn from_prev(p: DemoV0) -> Demo {
            Demo { a: p.a, ..Demo::default() }
        }
    }

    #[derive(Clone, Copy, serde::Deserialize)]
    struct DemoV0 {
        a: u16,
    }
    impl WireSize for DemoV0 {
        const SIZE: usize = varint_max_bytes(2);
    }
    impl Version for DemoV0 {
        const TAG: u16 = 1;
        type Prev = Base;
        type Wire = Self; // no append-only fields: reflexive `From`
        fn from_prev(_p: Base) -> DemoV0 {
            panic!("no prev")
        }
    }

    fn record_buf<T: Version>() -> [u8; 32] {
        assert!(1 + T::RECORD_MAX <= 32);
        [0u8; 32]
    }

    #[test]
    fn round_trips_and_stamps_varint_tag() {
        let d = Demo { a: 300, b: -7 };
        let mut buf = record_buf::<Demo>();
        let n = d.serialize_into(&mut buf).unwrap();
        assert_eq!(buf[0], 2); // varint tag 2, single byte
        assert_eq!(Demo::deserialize_from::<32>(&buf[..n]).unwrap(), d);
    }

    #[test]
    fn append_absent_field_normalizes_via_none() {
        // A record written before `b` was appended: tag + just the leading `a` (no Option tag byte).
        // Pass the EXACT record (unpadded) to prove `deserialize_from` pads internally.
        let mut buf = record_buf::<Demo>();
        let n = write_versioned(Demo::TAG, &(5u16,), &mut buf).unwrap();
        let out = Demo::deserialize_from::<32>(&buf[..n]).unwrap();
        assert_eq!(out.a, 5);
        assert_eq!(out.b, 99); // absent -> None (from the internal zero pad) -> default 99
    }

    #[test]
    fn stored_zero_is_not_a_sentinel() {
        // A genuine `0` (Some(0)) must survive as 0 — the whole point of using Option over sentinels.
        let d = Demo { a: 1, b: 0 };
        let mut buf = record_buf::<Demo>();
        let n = d.serialize_into(&mut buf).unwrap();
        assert_eq!(Demo::deserialize_from::<32>(&buf[..n]).unwrap().b, 0);
    }

    #[test]
    fn migrates_from_oldest() {
        let mut buf = record_buf::<Demo>();
        let n = write_versioned(DemoV0::TAG, &(7u16,), &mut buf).unwrap();
        assert_eq!(Demo::deserialize_from::<32>(&buf[..n]).unwrap(), Demo { a: 7, b: 99 });
    }

    #[test]
    fn unknown_tag_is_corrupt() {
        let mut buf = record_buf::<Demo>();
        let n = write_versioned(9, &(1u16,), &mut buf).unwrap();
        assert_eq!(Demo::deserialize_from::<32>(&buf[..n]), Err(Error::Corrupt));
    }

    // --- SHRINKING chain: latest `Small` is SMALLER than its prev `BigV0` (fields removed) ---
    // `Small` has no append-only fields, so it is its own wire (needs serde derives + WireSize).
    #[derive(Clone, Copy, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
    struct Small {
        a: u32,
    }
    impl WireSize for Small {
        const SIZE: usize = varint_max_bytes(4);
    }
    impl Version for Small {
        const TAG: u16 = 2;
        type Prev = BigV0;
        type Wire = Self;
        fn from_prev(p: BigV0) -> Small {
            Small { a: p.b } // drop a, c
        }
    }
    #[derive(Clone, Copy, serde::Deserialize)]
    #[allow(dead_code)] // a, c are decoded then dropped by the migration to `Small`
    struct BigV0 {
        a: u32,
        b: u32,
        c: u32,
    }
    impl WireSize for BigV0 {
        const SIZE: usize = 3 * varint_max_bytes(4);
    }
    impl Version for BigV0 {
        const TAG: u16 = 1;
        type Prev = Base;
        type Wire = Self;
        fn from_prev(_p: Base) -> BigV0 {
            panic!("no prev")
        }
    }

    #[test]
    fn record_max_folds_to_the_larger_prev_and_it_decodes() {
        // Latest is smaller, but RECORD_MAX is the MAX over the spine (= the big prev + tag).
        assert!(Small::SIZE < BigV0::SIZE);
        assert_eq!(Small::RECORD_MAX, BigV0::RECORD_MAX);

        let mut buf = record_buf::<Small>();
        let n = write_versioned(BigV0::TAG, &(111u32, 222u32, 333u32), &mut buf).unwrap();
        assert_eq!(Small::deserialize_from::<32>(&buf[..n]).unwrap(), Small { a: 222 });
    }
}

// Auto-sizing a version with a `fixed` field via the `max-size` feature: wrap only the fixed field
// in `WireFixed`, `#[derive(MaxSize)]`, and `RECORD_MAX` is inferred. `fixed` is a dev-dependency.
#[cfg(all(test, feature = "max-size"))]
mod maxsize_tests {
    use super::sizing::varint_max_bytes;
    use super::*;
    use crate::max_size::WireFixed;
    use fixed::types::U16F16;
    use postcard::experimental::max_size::MaxSize;

    #[derive(Clone, Copy, PartialEq, Debug)]
    struct Gain {
        gain: U16F16,
        n: u16,
    }
    impl Default for Gain {
        fn default() -> Self {
            Gain { gain: U16F16::from_num(1), n: 0 }
        }
    }
    #[derive(Clone, Copy, serde::Serialize, serde::Deserialize, MaxSize)]
    struct GainWire {
        gain: WireFixed<U16F16>, // only the fixed field is annotated
        n: u16,
    }
    impl From<GainWire> for Gain {
        fn from(w: GainWire) -> Gain {
            Gain { gain: w.gain.0, n: w.n }
        }
    }
    impl From<Gain> for GainWire {
        fn from(g: Gain) -> GainWire {
            GainWire { gain: WireFixed(g.gain), n: g.n }
        }
    }
    impl Version for Gain {
        const TAG: u16 = 1;
        type Prev = Base;
        type Wire = GainWire;
        fn from_prev(p: Base) -> Gain {
            match p {}
        }
    }

    #[test]
    fn fixed_field_autosizes_and_round_trips() {
        assert_eq!(<WireFixed<U16F16> as MaxSize>::POSTCARD_MAX_SIZE, 5);
        // RECORD_MAX is derived from the wire's `MaxSize` (no hand sum): 5 (fixed) + 3 (u16) + tag.
        assert_eq!(
            Gain::RECORD_MAX,
            <GainWire as MaxSize>::POSTCARD_MAX_SIZE + varint_max_bytes(core::mem::size_of::<u16>())
        );

        let g = Gain { gain: U16F16::from_num(3.5), n: 1000 };
        let mut buf = [0u8; 1 + Gain::RECORD_MAX];
        let n = g.serialize_into(&mut buf).unwrap();
        assert_eq!(Gain::deserialize_from::<{ 1 + Gain::RECORD_MAX }>(&buf[..n]).unwrap(), g);
    }
}

/// Compile-time validation: a chain whose tags do not strictly increase fails `_TAGS_INCREASE`,
/// and a `from_prev` whose parameter is not the declared `Prev` fails at the impl site.
///
/// ```compile_fail
/// use postcard_versioned::{Base, Version, WireSize};
/// #[derive(Clone, Copy, serde::Serialize, serde::Deserialize)] struct A { x: u8 }
/// impl WireSize for A { const SIZE: usize = 1; }
/// impl Version for A { const TAG: u16 = 5; type Prev = Base; type Wire = A; fn from_prev(p: Base) -> A { match p {} } }
/// #[derive(Clone, Copy, serde::Serialize, serde::Deserialize)] struct B { x: u8 }
/// impl WireSize for B { const SIZE: usize = 1; }
/// impl Version for B { const TAG: u16 = 5; type Prev = A; type Wire = B; fn from_prev(p: A) -> B { B { x: p.x } } }
/// let _ = B::deserialize_from::<32>(&[0u8; 8]); // B::TAG (5) not > A::TAG (5) -> const assert fails
/// ```
///
/// ```compile_fail
/// use postcard_versioned::{Base, Version, WireSize};
/// #[derive(Clone, Copy, serde::Serialize, serde::Deserialize)] struct A { x: u8 }
/// impl WireSize for A { const SIZE: usize = 1; }
/// impl Version for A { const TAG: u16 = 1; type Prev = Base; type Wire = A; fn from_prev(p: u8) -> A { A { x: p } } }
/// ```
#[cfg(doc)]
pub mod _compile_fail_docs {}
