//! `max-size` feature: derive [`WireSize`](crate::WireSize) from postcard's `MaxSize`, plus
//! [`WireFixed`] to bridge `fixed`-point fields (no existing crate provides this).
//!
//! Usage: enable the `max-size` feature, import the `MaxSize` trait/derive from `postcard`
//! directly (`use postcard::experimental::max_size::MaxSize;`), and add `#[derive(MaxSize)]` 
//! to wire types.
//!
//! For Fixed fields: wrap those fields in [`WireFixed`].

use postcard::experimental::max_size::MaxSize;

use crate::WireSize;
use crate::sizing::varint_max_bytes;

/// Any `MaxSize` type supplies [`WireSize`] automatically.
impl<T: MaxSize> WireSize for T {
    const SIZE: usize = <T as MaxSize>::POSTCARD_MAX_SIZE;
}

/// Transparent newtype bridging a `fixed`-point (or other inner-int) field to postcard's `MaxSize`,
/// which such foreign types cannot implement directly (orphan rule). Serializes exactly as `F`.
/// Wrap **only** the fixed fields in a wire struct; `#[derive(MaxSize)]` sizes the rest.
#[derive(Clone, Copy, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct WireFixed<F>(pub F);

impl<F> MaxSize for WireFixed<F> {
    // A fixed-point value serializes as its transparent inner integer (varint); `size_of` is the
    // inner width and `varint_max_bytes` a safe upper bound.
    const POSTCARD_MAX_SIZE: usize = varint_max_bytes(core::mem::size_of::<F>());
}
