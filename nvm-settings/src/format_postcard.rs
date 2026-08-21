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
