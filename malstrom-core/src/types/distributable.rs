use serde::{Serialize, de::DeserializeOwned};

use crate::types::Kvt;

/// A type which can be sent (distributed) between workers
pub trait Distributable: Serialize + DeserializeOwned + 'static {
    fn encode(self) -> Vec<u8>;

    fn decode(encoded: &[u8]) -> Self;
}
impl<T> Distributable for T
where
    T: Serialize + DeserializeOwned + 'static,
{
    fn encode(self) -> Vec<u8> {
        rmp_serde::encode::to_vec(&self).expect("Encoding error")
    }

    fn decode(encoded: &[u8]) -> Self {
        rmp_serde::decode::from_slice(encoded).expect("Decoding error")
    }
}

#[cfg(test)]
mod tests {
    use super::Distributable;

    /// Encode/decode round-trips for the primitive and container types users send
    /// between workers. A round-trip failing here would break every cross-worker
    /// channel (operator streams, coordinator req/res).
    #[test]
    fn round_trips_primitives() {
        for v in [0u64, 1, u64::MAX] {
            assert_eq!(u64::decode(&v.encode()), v);
        }
        for v in [0usize, 1, usize::MAX] {
            assert_eq!(usize::decode(&v.encode()), v);
        }
        for v in [true, false] {
            assert_eq!(bool::decode(&v.encode()), v);
        }
    }

    #[test]
    fn round_trips_containers() {
        let s = "hello malstrom".to_string();
        assert_eq!(String::decode(&s.clone().encode()), s);

        let bytes = vec![0u8, 1, 2, 255];
        assert_eq!(Vec::<u8>::decode(&bytes.clone().encode()), bytes);

        let nested = vec![(1u64, "a".to_string()), (2, "b".to_string())];
        assert_eq!(Vec::<(u64, String)>::decode(&nested.clone().encode()), nested);
    }

    /// The `Distributable` blanket impl covers tuples of distributable types, which is
    /// what `Kvt` streams are made of.
    #[test]
    fn round_trips_tuples() {
        let v = (7u64, 42i32, "x".to_string());
        assert_eq!(<(u64, i32, String)>::decode(&v.clone().encode()), v);
    }
}

#[cfg(test)]
mod proptests {
    use super::Distributable;
    use proptest::prelude::*;

    proptest! {
        /// Encode/decode is an identity for every generated value.
        #[test]
        fn round_trips_u64(v: u64) {
            prop_assert_eq!(u64::decode(&v.clone().encode()), v);
        }

        #[test]
        fn round_trips_i64(v: i64) {
            prop_assert_eq!(i64::decode(&v.clone().encode()), v);
        }

        #[test]
        fn round_trips_string(v: String) {
            prop_assert_eq!(String::decode(&v.clone().encode()), v);
        }

        #[test]
        fn round_trips_byte_vec(v: Vec<u8>) {
            prop_assert_eq!(Vec::<u8>::decode(&v.clone().encode()), v);
        }

        /// Nested tuples — the shape of a `Kvt` stream message.
        #[test]
        fn round_trips_kvt_tuple(v: (u64, i32, String)) {
            prop_assert_eq!(<(u64, i32, String)>::decode(&v.clone().encode()), v);
        }
    }
}
