//! Synchronous traits for lazily producing or consuming sequences of arbitrary length, and functions for piping producers into consumers.
//!
//! A [`Producer`](Producer) emits a sequence one item at a time, a generalized and buffered variation on the [`core::iter::Iterator`](core::iter::Iterator) trait. A [`BulkProducer`](BulkProducer) extends those capabilities with the option of producing multiple items at a time, yielding a generalized [`std::io::Read`](std::io::Read) abstraction.
//!
//! Dually, a [`Consumer`](Consumer) processes a sequence one item at a time. A [`BulkConsumer`](BulkConsumer) can further process multiple items at a time, yielding a generalized [`std::io::Write`](std::io::Write) abstraction.

use core::num::NonZeroUsize;
use core::mem::MaybeUninit;
use core::cmp::min;

use slice_n::Slice1;

use crate::SequenceState::{self, *};
use crate::PipeEnd;

/// A [`Consumer`](Consumer) consumes items one by one.
pub trait Consumer {
    /// The type of values that can be consumed an arbitrary number of times.
    type Repeated;
    /// The type of the last consumed value. Frequently but not necessarily the unit type `()`.
    type Last;
    /// The information by which the [`Consumer`](Consumer) can notify the calling code that no further items can be consumed. Frequently but not necessarily an error type.
    ///
    /// No trait methods may be called after any trait method returned one of these.
    type Stopped;

    /// Consume a single item.
    fn consume(&mut self, item: Self::Repeated) -> Option<Self::Stopped>;

    /// A [`Consumer`](Consumer) is allowed to store consumed data in a buffer without immediately processing
    /// it. This method triggers immediate processing of all currently buffered data.
    fn flush(&mut self) -> Option<Self::Stopped>;

    /// Notifies the [`Consumer`](Consumer) that no further items will need to be consumed. Performs a flush immediately after consuming the last item.
    ///
    /// No trait methods may be called after calling this one.
    fn close(&mut self, item: Self::Last) -> Option<Self::Stopped>;
}

/// A [`BulkConsumer`](BulkConsumer) is a [`Consumer`](Consumer) that can consume multiple items at a time.
///
/// The [`consumer_slots`](Self::consumer_slots) and [`did_consume`](Self::did_consume) methods provide the low-level mechanism for doing so. When a buffer from which the items should be consumed is available, the more convenient [`bulk_consume`](bulk_consume) function should be preferred.
pub trait BulkConsumer: Consumer where Self::Repeated: Copy {
    /// Returns a nonempty buffer into which items can be written, or indicates that no further items can be consumed.
    ///
    /// This method must be compatible with [`Self::consume`](Consumer::consume): when returning a slice of length `n`, the next `n` calls to [`Self::consume`](Consumer::consume) must return `None`.
    fn consumer_slots(&mut self) -> SequenceState<&mut Slice1<MaybeUninit<Self::Repeated>>, Self::Stopped>;

    /// Tells the [`BulkConsumer`](BulkConsumer) that some number of items has been written into it. This method may only be called after having placed at least as many items into the buffer returned by [`consumer_slots`](Self::consumer_slots). Violations of this contract may lead to undefined behavior, because the [`BulkConsumer`](BulkConsumer) is allowed to assume that these buffer slots then contain initialized memory.
    ///
    /// This method must change the state of the [`BulkConsumer`](BulkConsumer) exactly as if [`Self::consume`](Consumer::consume) had been called `amount` many times, with the first `amount` many items in the slice returned by the last call to [`Self::consumer_slots`](Self::consumer_slots) as arguments. For future calls to [`consumer_slots`](Self::consumer_slots) and [`did_consume`](Self::did_consume), the first `amount` many items of the slice are not considered to be part of the slice anymore. If this results in the (logical) slice to be empty, and [`did_consume`](Self::did_consume) is called again, it must behave as if the slice contained unspecified items.
    unsafe fn did_consume(&mut self, amount: NonZeroUsize);
}

/// The [`BulkConsumer`](BulkConsumer) consumes a non-zero number of items from the provided buffer, and
/// returns how many it has consumed.
pub fn bulk_consume<C, R, L, S>(c: &mut C, buffer: &Slice1<C::Repeated>) -> SequenceState<NonZeroUsize, C::Stopped>
where
    R: Copy,
    C: BulkConsumer<Repeated = R, Last = L, Stopped = S>,
{
    match c.consumer_slots() {
        Final(stopped) => Final(stopped),
        More(l) => {
            let amount = min(l.len_(), buffer.len_());
            MaybeUninit::write_slice(&mut l[..amount], &buffer[..amount]);
            unsafe {
                let amount = NonZeroUsize::new_unchecked(amount);
                c.did_consume(amount);
                More(amount)
            }
        }
    }
}

/// A [`Producer`](Producer) produces items one by one.
pub trait Producer {
    /// The type of values that can be produced an arbitrary number of times.
    type Repeated;
    /// The type of the last produced value. Frequently but not necessarily an error type.
    type Last;
    /// The information passed to the [`Producer`](Producer) when indicating that no further items needs to be produced. Frequently but not necessarily the unit type `()`.
    type Stopped;

    /// Produces a single item.
    ///
    /// No trait methods may be called after this one returned `SequenceState::Last(_)`.
    fn produce(&mut self) -> SequenceState<Self::Repeated, Self::Last>;

    /// A [`Producer`](Producer) is allowed to obtain data from some data source and buffer it even before that data
    /// is requested to be produced. This method instructs the `Producer` to move as much data from
    /// the data source into the internal buffer as possible.
    fn slurp(&mut self) -> ();

    /// Notifies the [`Producer`](Producer) that no further items will be requested.
    ///
    /// No trait methods may be called after calling this one.
    fn stop(&mut self, reason: Self::Stopped) -> ();
}

/// A [`BulkProducer`](BulkProducer) is a [`Producer`](Producer) that can produce multiple items at a time.
///
/// The [`producer_slots`](Self::producer_slots) and [`did_produce`](Self::did_produce) methods provide the low-level mechanism for doing so. When a buffer into which the items should be produced is available, the more convenient [`bulk_produce`](bulk_produce) function should be preferred.
pub trait BulkProducer: Producer where Self::Repeated: Copy {
    /// Returns a nonempty buffer from which items can be taken, or the last sequence item if it has been reached.
    ///
    /// This method must be compatible with [`Self::produce`](Producer::produce): when returning a slice of length `n`, the next `n` calls to [`Self::produce`](Producer::produce) have to return those `n` items.
    fn producer_slots(&self) -> SequenceState<&Slice1<Self::Repeated>, Self::Last>;

    /// Tells the [`BulkProducer`](BulkProducer) that some number of items has been taken from it.
    ///
    /// This method must change the state of the [`BulkProducer`](BulkProducer) exactly as if [`Self::produce`](Producer::produce) had been called `amount` many times. This also advances the compatibility requirement of the [`producer_slots`](Self::producer_slots) method.
    fn did_produce(&mut self, amount: NonZeroUsize);
}

/// The [`BulkProducer`](BulkProducer) produces a non-zero number of items into the provided buffer, and
/// returns how many it has produced. The memory in the buffer does not need to be initialized.
pub fn bulk_produce<P, R, L, S>(p: &mut P, buffer: &mut Slice1<MaybeUninit<P::Repeated>>) -> SequenceState<NonZeroUsize, P::Last>
where
    R: Copy,
    P: BulkProducer<Repeated = R, Last = L, Stopped = S>,
{
    match p.producer_slots() {
        Final(last) => return Final(last),
        More(r) => {
            let amount = min(r.len_(), buffer.len_());
            MaybeUninit::write_slice(&mut buffer[..amount], &r[..amount]);
            unsafe {
                let amount = NonZeroUsize::new_unchecked(amount);
                p.did_produce(amount);
                More(amount)
            }
        }
    }
}

/// Pipes all items from the [`Producer`](Producer) into the [`Consumer`](Consumer).
pub fn pipe<P, C, R, PL, CL, PS, CS>(p: &mut P, c: &mut C) -> PipeEnd<PL, CS> where
    P: Producer<Repeated = R, Last = PL, Stopped = PS>,
    C: Consumer<Repeated = R, Last = CL, Stopped = CS>,
{
    loop {
        match p.produce() {
            More(r) => match c.consume(r) {
                None => {}
                Some(cs) => return PipeEnd::ConsumerStopped(cs),
            }
            Final(pl) => return PipeEnd::ProducerLast(pl),
        }
    }
}

/// Pipes all items from the [`BulkProducer`](BulkProducer) into the [`BulkConsumer`](BulkConsumer) via the [`bulk_consume`](BulkConsumer::bulk_consume) method.
pub fn pipe_bulk_consume<P, C, R, PL, CL, PS, CS>(p: &mut P, c: &mut C) -> PipeEnd<PL, CS> where
    R: Copy,
    P: BulkProducer<Repeated = R, Last = PL, Stopped = PS>,
    C: BulkConsumer<Repeated = R, Last = CL, Stopped = CS>,
{
    loop {
        match p.producer_slots() {
            More(s) => match bulk_consume(c, s) {
                More(amount) => p.did_produce(amount),
                Final(e) => return PipeEnd::ConsumerStopped(e),
            }
            Final(e) => return PipeEnd::ProducerLast(e),
        }
    }
}

/// Pipes all items from the [`BulkProducer`](BulkProducer) into the [`BulkConsumer`](BulkConsumer) via the [`bulk_produce`](BulkProducer::bulk_produce) method.
pub fn pipe_bulk_produce<P, C, R, PL, CL, PS, CS>(p: &mut P, c: &mut C) -> PipeEnd<PL, CS> where
    R: Copy,
    P: BulkProducer<Repeated = R, Last = PL, Stopped = PS>,
    C: BulkConsumer<Repeated = R, Last = CL, Stopped = CS>,
{
    loop {
        match c.consumer_slots() {
            More(s) => match bulk_produce(p, s) {
                More(amount) => unsafe { c.did_consume(amount) },
                Final(e) => return PipeEnd::ProducerLast(e),
            }
            Final(e) => return PipeEnd::ConsumerStopped(e),
        }
    }
}
