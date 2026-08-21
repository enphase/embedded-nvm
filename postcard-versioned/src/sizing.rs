//! Private helpers for postcard max-serialized-size math.

/// Max bytes for a postcard **varint**.
pub(crate) const fn varint_max_bytes(int_bytes: usize) -> usize {
    (int_bytes * 8 + 6) / 7
}
