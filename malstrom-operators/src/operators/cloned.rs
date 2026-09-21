use malstrom_core::stream::StreamBuilder;
use malstrom_core::types::{Kvt, MaybeData, MaybeKey, MaybeTime, Sealed};

use super::split::Split;

/// Create multiple streams by cloning the message from a single stream.
///
/// This is a convenience wrapper over [`Split`]: it applies a broadcast partitioner
/// (every output receives every message) instead of taking a per-message routing
/// closure, so it offers no runtime capability [`Split::split`] lacks. It is kept as
/// the ergonomic, intention-revealing spelling of fan-out — `stream.cloned(name, 2)`
/// reads better than `stream.split(name, |_, outs| outs.fill(true), 2)` and needs no
/// closure. It is used by the `cloned_streams` example and the joining/splitting guide.
///
/// For per-message routing (a message reaching only some outputs) use [`Split`].
pub trait Cloned<In: Kvt>: Sealed {
    /// Create N new streams by copying all messages into every created stream.
    /// To partition the stream instead see [super::split::Split::const_split].
    ///
    /// # Parameters
    /// - `name`: base name for the fan-out operator; each output is named `<name>-<i>`.
    fn const_cloned<const N: usize>(self, name: impl Into<String>) -> [StreamBuilder<In>; N];

    /// Create N new streams by copying all messages into every created stream.
    /// To partition the stream instead see [super::split::Split::split].
    ///
    /// # Parameters
    /// - `name`: base name for the fan-out operator; each output is named `<name>-<i>`.
    /// - `outputs`: number of output streams to create at runtime.
    fn cloned(self, name: impl Into<String>, outputs: usize) -> Vec<StreamBuilder<In>>;
}

impl<In> Cloned<In> for StreamBuilder<In>
where
    In: Kvt,
{
    fn const_cloned<const N: usize>(self, name: impl Into<String>) -> [StreamBuilder<In>; N] {
        self.const_split(name, |_, outputs: &mut [bool; N]| {
            *outputs = [true; N];
        })
    }

    fn cloned(self, name: impl Into<String>, outputs: usize) -> Vec<StreamBuilder<In>> {
        self.split(name, |_, outs: &mut [bool]| outs.fill(true), outputs)
    }
}
