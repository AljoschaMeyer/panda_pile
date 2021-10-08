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

#![no_std]
#![feature(maybe_uninit_write_slice)]
#![feature(generic_associated_types)]

pub mod sync;
pub mod nb;
pub mod nb_send;
pub mod why;

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
