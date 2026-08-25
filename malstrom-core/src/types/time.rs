//! Types and traits specific to time-keeping and timestamped streams.

use serde::{Deserialize, Serialize};

/// Trait implemented by all types usable as timestamps in JetStream
pub trait Timestamp: PartialOrd + Ord + Clone + std::fmt::Debug + 'static {
    /// Maximum or final value of this type. This is the last possible timestamp.
    const MAX: Self;
    /// Minumum value of this type.
    const MIN: Self;

    /// Merges two timestamps. Merging is used to align timestamps coming from
    /// multiple source e.g. in keyed streams. Merging should yield the lowest
    /// common timestamp of the two values. For most types this will be equivalent
    /// to the minimum of the two values.
    fn merge(&self, other: &Self) -> Self;
}

/// Zero sized marker indicating a stream with no timestamps associated.
///
/// **IMPORTANT:** The NoTime type has a special meaning in JetStream:
/// Operators emittng `NoTime` are seen as not able to advance the computation.
/// This means if all operators emitting timestamps in a stream are finished, a `NoTime`
/// emitting operator will not keep the stream running.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoTime;

impl PartialOrd for NoTime {
    fn partial_cmp(&self, _other: &Self) -> Option<std::cmp::Ordering> {
        None
    }
}

/// A timestamp which can only either be finished or not, but nothing in between
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct OnceTime(bool);

impl Timestamp for OnceTime {
    const MAX: Self = Self(true);

    const MIN: Self = Self(false);

    fn merge(&self, other: &Self) -> Self {
        Self(self.0 & other.0)
    }
}

/// Time where the timestamp may not yet have been set
pub trait MaybeTime: std::fmt::Debug + Clone + PartialOrd + 'static {
    /// Try to merge two times, returning Some if the
    /// specific type implementing this trait implements
    /// Timestamp and None if it does not
    fn try_merge(&self, other: &Self) -> Option<Self>;

    /// Check if an optional timestamp is equal to the max timestamp
    /// This absolutely cursed implementation of a static function allows
    /// us to indicate that NoTime emitting operators are always done
    const CHECK_FINISHED: fn(&Option<Self>) -> bool;
}
impl<T> MaybeTime for T
where
    T: Timestamp + Clone + 'static,
{
    fn try_merge(&self, other: &Self) -> Option<Self> {
        Some(self.merge(other))
    }

    const CHECK_FINISHED: fn(&Option<Self>) -> bool =
        |opt_t| opt_t.as_ref().is_some_and(|t| *t == T::MAX);
}
impl MaybeTime for NoTime {
    fn try_merge(&self, _other: &Self) -> Option<Self> {
        Some(NoTime)
    }
    /// Always true, as a `NoTime` emitting Operator can not keep a stream running.
    const CHECK_FINISHED: fn(&Option<Self>) -> bool = |_| true;
}

/// Implements `Timestamp` for numeric types
macro_rules! timestamp_impl {
    ($t:ty) => {
        impl Timestamp for $t {
            const MAX: $t = <$t>::MAX;
            const MIN: $t = <$t>::MIN;

            fn merge(&self, other: &$t) -> $t {
                *self.min(other)
            }
        }
    };
}

timestamp_impl!(usize);
timestamp_impl!(u8);
timestamp_impl!(u16);
timestamp_impl!(u32);
timestamp_impl!(u64);
timestamp_impl!(u128);

timestamp_impl!(isize);
timestamp_impl!(i8);
timestamp_impl!(i16);
timestamp_impl!(i32);
timestamp_impl!(i64);
timestamp_impl!(i128);

#[cfg(test)]
mod tests {
    use super::{MaybeTime, NoTime, OnceTime, Timestamp};

    /// `merge` must yield the lowest common timestamp (min for the numeric impls)
    /// and be monotone: merging with a larger value never advances the result.
    #[test]
    fn usize_merge_is_min_and_monotone() {
        assert_eq!(5usize.merge(&3), 3);
        assert_eq!(3usize.merge(&5), 3);
        assert_eq!(7usize.merge(&7), 7);
        // merging any value with MAX keeps the smaller one
        assert_eq!(usize::MAX.merge(&1), 1);
    }

    /// `OnceTime` merges by logical AND: the stream is only finished once every
    /// input says so.
    #[test]
    fn once_time_merge_is_and() {
        assert_eq!(OnceTime::MAX.merge(&OnceTime::MIN), OnceTime::MIN);
        assert_eq!(OnceTime::MIN.merge(&OnceTime::MAX), OnceTime::MIN);
        assert_eq!(OnceTime::MAX.merge(&OnceTime::MAX), OnceTime::MAX);
        assert_eq!(OnceTime::MIN.merge(&OnceTime::MIN), OnceTime::MIN);
    }

    /// `CHECK_FINISHED` is only true for the max timestamp (numeric impls), so an
    /// operator emitting ordinary values never closes its stream early.
    #[test]
    fn check_finished_only_for_max() {
        let none: Option<usize> = None;
        assert!(!MaybeTime::CHECK_FINISHED(&none));
        assert!(!MaybeTime::CHECK_FINISHED(&Some(usize::MAX - 1)));
        assert!(MaybeTime::CHECK_FINISHED(&Some(usize::MAX)));
    }

    /// `NoTime` never compares and is always "finished" — the documented semantics
    /// for operators that cannot keep a stream running.
    #[test]
    fn no_time_semantics() {
        assert!(NoTime::partial_cmp(&NoTime, &NoTime).is_none());
        assert!(MaybeTime::CHECK_FINISHED(&None::<NoTime>));
    }
}

#[cfg(test)]
mod proptests {
    use super::Timestamp;
    use proptest::prelude::*;

    /// `merge` must be a commutative, associative, idempotent meet (min for the
    /// numeric impls), and monotone: merging with a larger value never advances.
    proptest! {
        #[test]
        fn usize_merge_is_min(a: usize, b: usize) {
            prop_assert_eq!(a.merge(&b), a.min(b));
        }

        #[test]
        fn merge_commutative(a: u64, b: u64) {
            prop_assert_eq!(a.merge(&b), b.merge(&a));
        }

        #[test]
        fn merge_associative(a: u64, b: u64, c: u64) {
            prop_assert_eq!(a.merge(&b).merge(&c), a.merge(&b.merge(&c)));
        }

        #[test]
        fn merge_idempotent(a: u64) {
            prop_assert_eq!(a.merge(&a), a);
        }

        #[test]
        fn merge_monotone(a: u64, b: u64) {
            // merging can only advance the frontier downward (towards completion)
            prop_assert!(a.merge(&b) <= a);
            prop_assert!(a.merge(&b) <= b);
        }
    }
}
