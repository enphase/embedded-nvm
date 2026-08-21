use crate::SettingsFormat;
use postcard_versioned::Version;

/// [`SettingsFormat`] using [`postcard_versioned`] for versioned, migratable
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
    type Error = postcard_versioned::Error;

    fn serialize(&self, value: &T, buf: &mut [u8]) -> Result<usize, Self::Error> {
        value.serialize_into(buf)
    }

    fn deserialize(&self, record: &[u8]) -> Result<T, Self::Error> {
        T::deserialize_from::<BUF_SIZE>(record)
    }
}
