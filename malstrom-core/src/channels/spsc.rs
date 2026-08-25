//! A dead simple non-threaded unbounded channel
//! Inspiration taken from https://docs.rs/local-channel

use std::{
    cell::RefCell,
    collections::VecDeque,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

use futures::Stream;
use pin_project::pin_project;

type Shared<T> = Rc<RefCell<SharedInner<T>>>;

// TODO: Make this configurable
static CAPACITY: usize = 1024;

#[derive(Debug)]
struct SharedInner<T> {
    queue: VecDeque<T>,
    capacity: usize,
    has_receiver: bool,
    recv_waker: Option<Waker>,
    send_waker: Option<Waker>,
    /// woken when the receiver is dropped (used to detect downstream termination)
    receiver_gone: Option<Waker>,
}
impl<T> SharedInner<T> {
    /// Get a reference to the last value if any
    /// without removing it
    fn peek(&self) -> Option<&T> {
        self.queue.front()
    }
}
impl<T> Default for SharedInner<T> {
    fn default() -> Self {
        Self {
            queue: Default::default(),
            has_receiver: Default::default(),
            capacity: CAPACITY,
            recv_waker: None,
            send_waker: None,
            receiver_gone: None,
        }
    }
}

/// A sender for sending messages into the channel
#[derive(Debug)]
pub struct Sender<T> {
    shared: Shared<T>,
}
impl<T> Sender<T> {
    /// Send a message into the channel. Note that sending
    /// to a channel without any receiver drops the message
    pub fn send(&self, msg: T) -> Send<'_, T> {
        Send {
            sender: &self,
            value: RefCell::new(Some(msg)),
        }
    }

    /// Send a message without respecting the capacity
    pub(crate) fn force_send(&self, msg: T) {
        let mut shared = self.shared.borrow_mut();
        shared.queue.push_back(msg);
        if let Some(waker) = shared.recv_waker.take() {
            waker.wake();
        }
    }

    /// Future which completes once the receiver of this channel has been dropped
    pub(crate) fn wait_receiver_gone(&self) -> ReceiverGone<'_, T> {
        ReceiverGone {
            sender: self,
            was_alive: false,
        }
    }
}

/// Future which completes once the channel's receiver has been dropped.
/// If the receiver was already gone on the first poll (e.g. an unused tail
/// receiver that was dropped during build), the future never resolves.
pub struct ReceiverGone<'a, T> {
    sender: &'a Sender<T>,
    was_alive: bool,
}

impl<'a, T> Future for ReceiverGone<'a, T> {
    type Output = ();

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<()> {
        let mut shared = self.sender.shared.borrow_mut();
        if !self.was_alive {
            // first poll: only start watching if a receiver is actually attached
            self.was_alive = shared.has_receiver;
            if !shared.has_receiver {
                return Poll::Pending;
            }
        }
        if !shared.has_receiver {
            Poll::Ready(())
        } else {
            shared.receiver_gone = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

pub struct Send<'a, T> {
    sender: &'a Sender<T>,
    value: RefCell<Option<T>>,
}

impl<'a, T> Future for Send<'a, T> {
    type Output = ();

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let mut shared = self.sender.shared.borrow_mut();
        if !shared.has_receiver {
            // The receiver is gone (e.g. the terminal operator's output, whose tail
            // receiver is dropped at build time) — drop the message instead of
            // queueing it. Queuing would eventually fill the bounded channel and
            // block the upstream operator forever.
            self.value.take();
            return Poll::Ready(());
        }
        if shared.capacity > shared.queue.len() {
            if let Some(v) = self.value.take() {
                shared.queue.push_back(v);
            }
            // wake up receiver
            shared.recv_waker.take().map(Waker::wake);
            Poll::Ready(())
        } else {
            // wake any receiver to free up the queue
            shared.recv_waker.take().map(Waker::wake);
            // set a waker so we can try again when the receiver frees up the queue
            shared.send_waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

/// A receiver for receiving messages from the channel
#[derive(Debug)]
pub struct Receiver<T> {
    shared: Shared<T>,
}
impl<T> Receiver<T> {
    fn new(shared: Shared<T>) -> Self {
        shared.borrow_mut().has_receiver = true;
        Self { shared }
    }
}
impl<T> super::recv_trait::Receiver for Receiver<T> {
    type Output = T;
    /// Receive a message from the channel, returns None if the channel
    /// contains no messages
    fn recv(&mut self) -> Receive<'_, T> {
        Receive(self)
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut shared = self.shared.borrow_mut();
        shared.has_receiver = false;
        if let Some(waker) = shared.receiver_gone.take() {
            waker.wake();
        }
    }
}

pub struct Receive<'a, T>(&'a Receiver<T>);

impl<'a, T> Future for Receive<'a, T> {
    type Output = T;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let mut shared = self.0.shared.borrow_mut();
        match shared.queue.pop_front() {
            Some(x) => {
                // tell any waiting sender there is space in queue
                shared.send_waker.take().map(Waker::wake);
                Poll::Ready(x)
            }
            None => {
                // let sender know we are waiting for a message
                shared.recv_waker = Some(cx.waker().clone());
                debug_assert!({
                    // 2: One sender, one receiver
                    // <2: Only receiver (this one) left
                    drop(shared);
                    Rc::strong_count(&self.0.shared) <= 2
                });
                Poll::Pending
            }
        }
    }
}

pub fn unbounded<T>() -> (Sender<T>, Receiver<T>) {
    let shared = Rc::new(RefCell::new(SharedInner::default()));
    let sender = Sender {
        shared: shared.clone(),
    };
    let receiver = Receiver::new(shared);
    (sender, receiver)
}

#[cfg(test)]
mod tests {
    use crate::channels::recv_trait::Receiver as _;

    use super::*;

    /// If sending without a receiver the message should be dropped
    #[test]
    fn sending_without_receiver() {
        let foo = Rc::new(42);
        let (tx, _) = unbounded();
        tx.send(foo.clone());
        // we should be able to unwrap since the other reference
        // was dropped due to no receiver
        assert!(Rc::try_unwrap(foo).is_ok())
    }

    /// Send a message and receive it
    #[tokio::test]
    async fn send_and_receive() {
        let (tx, mut rx) = unbounded();
        tx.send("HelloWorld").await;
        assert_eq!(rx.recv().await, "HelloWorld")
    }

    /// Sends and receives messages in the correct
    /// order
    #[tokio::test]
    async fn send_and_receive_order() {
        let (tx, mut rx) = unbounded();
        tx.send("HelloWorld").await;
        tx.send("FooBar").await;
        assert_eq!(rx.recv().await, "HelloWorld");
        assert_eq!(rx.recv().await, "FooBar");
    }

    /// copied from https://users.rust-lang.org/t/a-macro-to-assert-that-a-type-does-not-implement-trait-bounds/31179
    macro_rules! assert_not_impl {
        ($x:ty, $($t:path),+ $(,)*) => {
            const _: fn() -> () = || {
                struct Check<T: ?Sized>(T);
                trait AmbiguousIfImpl<A> { fn some_item() { } }

                impl<T: ?Sized> AmbiguousIfImpl<()> for Check<T> { }
                impl<T: ?Sized $(+ $t)*> AmbiguousIfImpl<u8> for Check<T> { }

                <Check::<$x> as AmbiguousIfImpl<_>>::some_item()
            };
        };
    }

    /// Check neither the sender nor receiver implement Clone or Copy, making them
    /// SPSC
    #[test]
    fn is_spsc() {
        assert_not_impl!(Sender<()>, Copy);
        assert_not_impl!(Sender<()>, Clone);
        assert_not_impl!(Receiver<()>, Copy);
        assert_not_impl!(Receiver<()>, Clone);
    }
}
