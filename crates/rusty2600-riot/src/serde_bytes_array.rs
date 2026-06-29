use core::convert::TryInto;
use serde::{Deserializer, Serializer, de};

pub fn serialize<S: Serializer, const N: usize>(
    data: &[u8; N],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeTuple;
    let mut seq = serializer.serialize_tuple(N)?;
    for b in data.iter() {
        seq.serialize_element(b)?;
    }
    seq.end()
}

pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(
    deserializer: D,
) -> Result<[u8; N], D::Error> {
    struct Visitor<const N: usize>;
    impl<'de, const N: usize> de::Visitor<'de> for Visitor<N> {
        type Value = [u8; N];
        fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
            write!(formatter, "a byte array of length {}", N)
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut arr = [0u8; N];
            for i in 0..N {
                arr[i] = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(i, &self))?;
            }
            Ok(arr)
        }
    }
    deserializer.deserialize_tuple(N, Visitor::<N>)
}
