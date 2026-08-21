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
