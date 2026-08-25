//! The keyed state-movement protocol messages. These ride in the `Message` enum
//! and the `SafeLogic` handlers, so they are part of the kernel's runtime
//! vocabulary; the router machinery that moves them lives in the
//! `malstrom-distributed` crate.

use std::{cell::RefCell, hash::Hash, marker::PhantomData, rc::Rc};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::{
    snapshot::{deserialize_state, serialize_state},
    types::{Kvt, OperatorId, distributable::Distributable},
};

/// Acquire encapsulates state which has moved to the current worker from another worker due to
/// a reconfiguration
#[derive(Clone, Debug)]
pub struct Acquire<K> {
    inner: Rc<RefCell<(K, IndexMap<OperatorId, Vec<u8>>)>>,
}

impl<K> Acquire<K> {
    /// Create a new [Acquire] carrying state for the given key
    pub fn new(key: K, collection: IndexMap<OperatorId, Vec<u8>>) -> Self {
        Self {
            inner: Rc::new(RefCell::new((key, collection))),
        }
    }
}

impl<K> Acquire<K>
where
    K: Distributable + Clone,
{
    /// Take the moved state for a given order from this [Acquire]
    pub fn take_state<S: Distributable>(&self, operator_id: &OperatorId) -> Option<(K, S)> {
        let mut inner = self.inner.borrow_mut();
        match inner.1.swap_remove(operator_id) {
            Some(state) => Some((inner.0.clone(), deserialize_state(state))),
            None => None,
        }
    }
}

/// The Collect messages takes state from operators so it can be sent to another worker
#[derive(Clone)]
pub struct Collect<K> {
    key: K,
    backchannel: tokio::sync::mpsc::UnboundedSender<(OperatorId, Vec<u8>)>,
}

impl<K> Collect<K>
where
    K: Hash + Eq,
{
    /// Create a new [Collect] for the given key; the returned receiver collects the
    /// states handed to [Collect::add_state].
    pub fn new(
        key: K,
    ) -> (
        Self,
        tokio::sync::mpsc::UnboundedReceiver<(OperatorId, Vec<u8>)>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (
            Self {
                key,
                backchannel: tx,
            },
            rx,
        )
    }

    /// Add a state for the [Collect]'s key. The operator MUST not use the state or a clone of it
    /// after giving it to this method.
    ///
    /// The correct key can be obtained from [Collect::get_key]
    pub fn add_state<S: Distributable>(&self, operator_id: OperatorId, state: &S) {
        let serialized = serialize_state(state);
        self.backchannel
            .send((operator_id, serialized))
            .expect("Expected Collect to be alive")
    }

    /// The key whose state this [Collect] gathers.
    pub fn get_key(&self) -> &K {
        &self.key
    }
}

/// The Interrogate message is passed along a stream to identify which keys have associated state
#[derive(Clone)]
pub struct Interrogate<K> {
    sender: tokio::sync::mpsc::UnboundedSender<K>,
    _key_type: PhantomData<K>,
}

impl<K> Interrogate<K>
where
    K: Distributable,
{
    /// Create a new [Interrogate]; the returned receiver collects the keys reported via
    /// [Interrogate::add_keys].
    pub fn new() -> (Self, tokio::sync::mpsc::UnboundedReceiver<K>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (
            Self {
                sender: tx,
                _key_type: PhantomData,
            },
            rx,
        )
    }

    /// Inform this [Interrogate] about multiple keys for which this operator has state
    pub fn add_keys(&self, keys: impl IntoIterator<Item = K>) {
        for k in keys.into_iter() {
            self.sender
                .send(k)
                .expect("Expected Interrogator to be alive")
        }
    }
}
