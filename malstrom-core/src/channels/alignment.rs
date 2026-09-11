use std::hash::Hash;

use futures::{StreamExt, stream::FuturesUnordered};
use indexmap::IndexMap;

/// A group of [Receiver]s which will pause each receiver when the last message received
/// satisfies a given condition.
/// The receiver is unpaused once all receivers have met the condition.
/// Messages satisfying the condition are not immediatly emitted, but instead all emitted once
/// all receivers have met the condition. The order in which the paused messages are emitted is
/// **not specified**
pub struct AlignmentGroup<K, R: super::recv_trait::Receiver, F> {
    receivers: IndexMap<K, AlignedReceiver<R>>,
    condition: F,
}

struct AlignedReceiver<R: super::recv_trait::Receiver> {
    receiver: R,
    /// double duty as flag whether the receiver is paused and contains paused message
    paused: Option<R::Output>,
}

impl<K, R, F> AlignmentGroup<K, R, F>
where
    K: Hash + Eq,
    R: super::recv_trait::Receiver,
    F: Fn(&R::Output) -> bool,
{
    /// Create a new AlignmentGroup with the given receivers and condition function
    pub fn new(receivers: impl IntoIterator<Item = (K, R)>, condition: F) -> Self {
        let aligned_receivers = receivers
            .into_iter()
            .map(|(key, receiver)| {
                (
                    key,
                    AlignedReceiver {
                        receiver,
                        paused: None,
                    },
                )
            })
            .collect();

        Self {
            receivers: aligned_receivers,
            condition,
        }
    }

    /// Create a new empty AlignmentGroup with the given condition function
    pub fn new_empty(condition: F) -> Self {
        Self {
            receivers: IndexMap::new(),
            condition,
        }
    }

    /// Add a new receiver to the AlignmentGroup
    pub fn insert(&mut self, key: K, receiver: R) {
        self.receivers.insert(
            key,
            AlignedReceiver {
                receiver,
                paused: None,
            },
        );
    }

    /// Remove a receiver from the [AlignmentGroup]
    pub fn remove(&mut self, key: &K) {
        let _ = self.receivers.swap_remove(key);
    }

    /// Retain only keys for which `keep` returns true.
    pub fn retain(&mut self, mut keep: impl FnMut(&K) -> bool) {
        self.receivers.retain(|k, _| keep(k));
    }

    /// Get the set of keys in this alignmentgroup
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.receivers.keys()
    }

    pub fn values(&self) -> impl Iterator<Item = &R> {
        self.receivers.values().map(|x| &x.receiver)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut R> {
        self.receivers.get_mut(key).map(|x| &mut x.receiver)
    }
}

impl<K, R, F> super::recv_trait::Receiver for AlignmentGroup<K, R, F>
where
    K: Clone,
    R: super::recv_trait::Receiver,
    F: Fn(&R::Output) -> bool,
{
    type Output = AlignedValue<K, R::Output>;

    async fn recv(&mut self) -> Self::Output {
        let mut recv_futures: FuturesUnordered<_> = self
            .receivers
            .iter_mut()
            .filter(|(_, x)| x.paused.is_none())
            .map(|(k, v)| async move {
                let msg = v.receiver.recv().await;
                (k, v, msg)
            })
            .collect();

        if recv_futures.is_empty() {
            std::future::pending::<()>().await;
        }

        loop {
            // TODO: left biased
            // ... is this the place to intertwine united values?
            match recv_futures.next().await {
                Some((key, aligned_receiver, msg)) => {
                    if (self.condition)(&msg) {
                        aligned_receiver.paused = Some(msg);
                        continue;
                    }
                    return AlignedValue::Unaligned((key.clone(), msg));
                }
                // no unblocked futures or self.receivers is empty
                None => {
                    drop(recv_futures);
                    let aligned_values = self
                        .receivers
                        .iter_mut()
                        .map(|(key, x)| {
                            (
                                key.clone(),
                                x.paused.take().expect("Paused message to be saved"),
                            )
                        })
                        .collect();
                    return AlignedValue::Aligned(aligned_values);
                }
            }
        }
    }
}

pub enum AlignedValue<K, T> {
    /// Individual value of T, does not need alignment
    /// and index of channel this value came from
    Unaligned((K, T)),
    /// Multiple values which were aligned
    Aligned(Vec<(K, T)>),
}

#[cfg(test)]
mod tests {
    use super::{AlignedValue, AlignmentGroup};
    use crate::channels::{recv_trait::Receiver as _, spsc};

    /// A condition on the payload: values equal to the sentinel "pause" the channel.
    fn is_barrier(v: &u64) -> bool {
        *v == u64::MAX
    }

    /// A non-barrier message on a single channel passes through immediately.
    #[tokio::test]
    async fn unaligned_passes_through() {
        let (tx, rx) = spsc::unbounded();
        let mut group: AlignmentGroup<usize, spsc::Receiver<u64>, _> =
            AlignmentGroup::new([(0usize, rx)], is_barrier);
        tx.send(1).await;
        let v = group.recv().await;
        assert!(matches!(v, AlignedValue::Unaligned((0, 1))));
    }

    /// A barrier on one channel is held until every channel has reported a barrier,
    /// then all barriers are emitted together.
    #[tokio::test]
    async fn barrier_held_until_all_channels_report() {
        let (tx0, rx0) = spsc::unbounded();
        let (tx1, rx1) = spsc::unbounded();
        let mut group: AlignmentGroup<usize, spsc::Receiver<u64>, _> =
            AlignmentGroup::new([(0usize, rx0), (1, rx1)], is_barrier);

        // only one channel barred -> recv must not complete within the grace period
        tx0.send(u64::MAX).await;
        let timed_out = tokio::time::timeout(std::time::Duration::from_millis(50), group.recv())
            .await
            .is_err();
        assert!(
            timed_out,
            "barrier must be held until all channels are barred"
        );

        // now both channels are barred -> all barriers are emitted at once
        tx1.send(u64::MAX).await;
        let v = group.recv().await;
        match v {
            AlignedValue::Aligned(items) => {
                assert_eq!(
                    items.len(),
                    2,
                    "both paused barriers must be emitted together"
                );
                assert!(items.iter().all(|(_, x)| *x == u64::MAX));
            }
            _ => panic!("expected aligned barriers"),
        }
    }

    /// After alignment the group can receive again.
    #[tokio::test]
    async fn alignment_recovers() {
        let (tx0, rx0) = spsc::unbounded();
        let (tx1, rx1) = spsc::unbounded();
        let mut group: AlignmentGroup<usize, spsc::Receiver<u64>, _> =
            AlignmentGroup::new([(0usize, rx0), (1, rx1)], is_barrier);
        tx0.send(u64::MAX).await;
        tx1.send(u64::MAX).await;
        assert!(matches!(group.recv().await, AlignedValue::Aligned(_)));

        tx0.send(9).await;
        assert!(matches!(
            group.recv().await,
            AlignedValue::Unaligned((0, 9))
        ));
    }
}
