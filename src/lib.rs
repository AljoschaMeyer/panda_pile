//! # Panda Pile
//!
//! Traits for lazily producing or consuming sequences of arbitrary length. This crate provides unified, synchronous and asynchronous alternatives to the [`core::iter::Iterator`](core::iter::Iterator), [`std::io::Read`](std::io::Read), and [`std::io::Write`](std::io::Write) traits, offering the following improvements:
//!
//! - duality between producing and consuming APIs
//! - completely analogous synchronous and asynchronous APIs
//! - bulk production/consumption as a subtrait of individual production/consumption
//! - generic item and error types
//! - dedicated type for the last sequence item
//! - no_std
//!
//! A derivation of the APIs from first principles is given [here](why), the remainder of this documentation focuses on the *what* and *how* rather than the *why*. This crate only provides traits, see the [panda_party](TODO) crate for useful implementations such as combinators or adapters from and to other sequence abstractions.
//!
//! All traits come in three versions: synchronous, blocking in the [`sync`](sync) module; nonblocking in the [`nb`](nb) module; and nonblocking, threadsafe for scheduling on a multithreaded executor in the [`nb_send`](nb_send) module.
//!
//! ## Nonblocking Traits
//!
//! The nonblocking APIs do not use async functions, both because as of writing rust does not support [`async fn` in traits](https://smallcultfollowing.com/babysteps/blog/2019/10/26/async-fn-in-traits-are-hard/), and because as of writing there is no way to obtain the type of the [`Future`](core::future::Future) returned by an `async fn`, severely hindering most nontrivial library code. Instead, each trait has an associated type per method, this type is a [`Future`](core::future::Future) that performs the operation. The actual trait methods take a pinned, mutable reference to `Self` and return a pinned, mutable reference to the corresponding [`Future`](core::future::Future). An abstract example:
//!
//! ```rust
//! trait ExampleSynchronousTrait {
//!     fn method(&mut self, foo: String) -> u32;
//! }
//!
//! trait CorrespondingAsynchronousTrait {
//!     type Method: Future<Output = u32>;
//!     fn method(self: Pin<&mut Self>, foo: String) -> Pin<&mut Self::Method>;
//! }
//! ```
//!
//! All nonblocking traits in this crate are derived from a corresponding synchronous trait in this manner. Threadsafe nonblocking traits additionally have a `+ Send` bound on their associated types.
//!
//! Because of the borrowing rules, this approach to asynchronous traits ensures that at most one of the associated futures can exist at a time. It is thus impossible to poll multiple of the associated futures concurrently. Consequently, the API contracts for nonblocking traits can stay almost identical to those of the corresponding synchronous traits. The only way to obtain behavior from a nonblocking trait that would be impossible to obtain from the corresponding synchronous trait is to begin polling an associated future but dropping it before it completes. Because trait implementations should not be forced to handle this cornercase, part of the API contract for all nonblocking traits is that no trait methods may be called after dropping any associated future that has not been polled to completion.

#![cfg_attr(not(feature = "std"), feature(no_std))]
#![feature(maybe_uninit_write_slice)]
#![feature(maybe_uninit_slice)]
#![feature(maybe_uninit_extra)]
#![feature(maybe_uninit_uninit_array)]
#![feature(generic_associated_types)]
#![cfg_attr(all(any(feature = "alloc", feature = "std"), feature = "arbitrary"), feature(new_uninit))]
#![cfg_attr(any(feature = "alloc", feature = "std"), feature(allocator_api))]
#![feature(never_type)]

pub mod sync;
pub mod nb;
pub mod nb_send;
pub mod why;

use core::task::Poll;
use core::future::Future;
use core::pin::Pin;
use core::num::NonZeroUsize;
use core::mem::MaybeUninit;
use core::mem::transmute;
use core::task::Context;
// use core::cmp::min;

use either::Either;

use slice_n::Slice1;

/// A sequence of arbitrary length is considered to consist of an arbitrary number of items of some type `R` followed by (at most) one item of type `L`. A `SequenceState<R, L>` indicates whether its contained value belongs to the part of the sequence that can be repeated an arbitrary number of times, or whether the last item of the sequence has been reached.
pub enum SequenceState<R, L> {
    /// More sequence items may follow.
    More(R),
    /// No more sequence items will follow.
    Final(L),
}

/// Indicates why piping data from a producer into a consumer ended.
pub enum PipeEnd<PL, CS> {
    /// The producer produced its last item.
    ProducerLast(PL),
    /// The consumer stopped accepting more data.
    ConsumerStopped(CS),
}


// 
// use SequenceState::*;
//
// pub trait Producer {
//     type Repeated;
//     type Last;
//     type Stopped;
//
//     type Produce: Future<Output = SequenceState<Self::Repeated, Self::Last>>;
//     type Stop: Future<Output = ()>;
//     type Slurp: Future<Output = ()>;
//
//     fn produce(self: Pin<&mut Self>) -> Pin<&mut Self::Produce>;
//     fn stop(self: Pin<&mut Self>, reason: Self::Stopped) -> Pin<&mut Self::Stop>;
//     fn slurp(self: Pin<&mut Self>) -> Pin<&mut Self::Slurp>;
// }
//
// /// Produces data from a slice.
// pub struct CursorP<'a, T>(&'a [T], usize);
// pub struct CursorProduce<'a, T>(CursorP<'a, T>);
// pub struct CursorStop<'a, T>(CursorP<'a, T>);
// pub struct CursorSlurp<'a, T>(CursorP<'a, T>);
//
// impl<'a, T: Clone> Future for CursorProduce<'a, T> {
//     type Output = SequenceState<T, ()>;
//
//     fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
//         let s = &mut Pin::into_inner(self).0;
//         if s.0.len() == s.1 + 1 {
//             Poll::Ready(Last(()))
//         } else {
//             s.1 += 1;
//             Poll::Ready(Repeated(s.0[s.1 - 1].clone()))
//         }
//     }
// }
//
// impl<'a, T: Clone> Future for CursorStop<'a, T> {
//     type Output = ();
//
//     fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
//         unimplemented!()
//     }
// }
//
// impl<'a, T: Clone> Future for CursorSlurp<'a, T> {
//     type Output = ();
//
//     fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
//         unimplemented!()
//     }
// }
//
// impl<'a, T: Clone> Producer for CursorP<'a, T> {
//     type Repeated = T;
//     type Last = ();
//     type Stopped = ();
//
//     type Produce = CursorProduce<'a, T>;
//     type Stop = CursorStop<'a, T>;
//     type Slurp = CursorSlurp<'a, T>;
//
//     fn produce(self: Pin<&mut Self>) -> Pin<&mut Self::Produce> {
//         unsafe { self.map_unchecked_mut(|p| transmute(p)) }
//     }
//
//     fn stop(self: Pin<&mut Self>, _reason: Self::Stopped) -> Pin<&mut Self::Stop> {
//         unsafe { self.map_unchecked_mut(|p| transmute(p)) }
//     }
//     fn slurp(self: Pin<&mut Self>) -> Pin<&mut Self::Slurp> {
//         unsafe { self.map_unchecked_mut(|p| transmute(p)) }
//     }
// }
//
// pub trait Consumer {
//     type Repeated;
//     type Last;
//     type Stopped;
//
//     type Consume: Future<Output = Option<Self::Stopped>>;
//     type Close: Future<Output = Option<Self::Stopped>>;
//     type Flush: Future<Output = Option<Self::Stopped>>;
//
//     fn consume(self: Pin<&mut Self>, item: Self::Repeated) -> Pin<&mut Self::Consume>;
//     fn close(self: Pin<&mut Self>, item: Self::Last) -> Pin<&mut Self::Close>;
//     fn flush(self: Pin<&mut Self>) -> Pin<&mut Self::Flush>;
// }
//
// pub async fn pipe<P, C, R, L, SP, SC>(mut p: Pin<&mut P>, mut c: Pin<&mut C>) -> Option<SC> where
//     P: Producer<Repeated = R, Last = L, Stopped = SP>,
//     C: Consumer<Repeated = R, Last = L, Stopped = SC>,
// {
//     loop {
//         match p.as_mut().produce().await {
//             Repeated(r) => match c.as_mut().consume(r).await {
//                 None => {}
//                 Some(sc) => return Some(sc),
//             }
//             Last(l) => return c.close(l).await,
//         }
//     }
// }
//
// pub trait BulkProducer: Producer where Self::Repeated: Copy {
//     type Slots<'s>: Future<Output = SequenceState<&'s Slice1<Self::Repeated>, Self::Last>> where Self: 's;
//     // type DidProduce: Future<Output = Option<Self::Stopped>>;
//
//     fn producer_slots(&self) -> Self::Slots<'_>;
//     fn did_produce(&mut self, amount: NonZeroUsize);
//
//     fn bulk_produce(&mut self, buffer: &mut Slice1<MaybeUninit<Self::Repeated>>) -> SequenceState<usize, Self::Last> {
//         unimplemented!()
//         // match self.producer_slots() {
//         //     Last(last) => return Last(last),
//         //     Repeated(r) => {
//         //         let amount = min(r.len_(), buffer.len_());
//         //         MaybeUninit::write_slice(&mut buffer[..amount], &r[..amount]);
//         //         unsafe {
//         //             let amount = NonZeroUsize::new_unchecked(amount);
//         //             self.did_produce(amount);
//         //             Ok(amount)
//         //         }
//         //     }
//         // }
//     }
// }

// pub trait BulkConsumer: Consumer where Self::Repeated: Copy {
//     fn consumer_slots(&mut self) -> SequenceState<&mut Slice1<MaybeUninit<Self::Repeated>>, Self::Stopped>;
//     unsafe fn did_consume(&mut self, amount: NonZeroUsize);
//
//     fn bulk_consume(&mut self, buffer: &Slice1<Self::Repeated>) -> SequenceState<NonZeroUsize, Self::Stopped> {
//         unimplemented!()
//         // match self.consumer_slots()? {
//         //     Last(stopped) => Last(stopped),
//         //     Repeated(l) => {
//         //         let amount = min(l.len_(), data.len_());
//         //         MaybeUninit::write_slice(&mut l[..amount], &data[..amount]);
//         //         unsafe {
//         //             let amount = NonZeroUsize::new_unchecked(amount);
//         //             self.did_consume(amount);
//         //             Ok(amount)
//         //         }
//         //     }
//         // }
//     }
// }











#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
