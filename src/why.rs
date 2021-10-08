//! Reasoning behind the API design.
//!
//! ## Producers and Consumers
//!
//! The aim of the panda pile is to provide abstractions for robust communication between systems. Systems can communicate via a sequence of data pieces, and a robust system should be able to handle any well-formed sequence. In particular, these sequences might have arbitrary length, for example a web server cannot know in advance how many requests it will receive on a particular connection.
//!
//! Every computer system has only a finite amount of memory available, which limits the computations that can be performed. If the system needs to be able to handle arbitrarily long data sequences as input, it cannot store the whole sequence in memory at once. Instead, the sequence needs to be processed over time, reusing the space that was used to process past data items.
//!
//! Our core abstractions will thus be abstractions for working with sequences of arbitrary, possibly even infinite (at least conceptually) length. A first attempt for such an abstraction could look as follows:
//!
//! ```rust
//! // A `Producer1` produces items one by one.
//! trait Producer1 {
//!     // The type of values that are produced by the `Producer1`.
//!     type Item;
//!
//!     // Produces a single item.
//!     fn produce(&mut self) -> Self::Repeated;
//! }
//! ```
//!
//! Such a `Producer1` represents a stream of incoming data. Code working with it would repeatedly ask it to `produce` the next piece of data, processing it before asking for the next one. Control over the rate at which items are produced lies in the code working with the producer, not with the producer itself. This is essential for robust solutions, as eagerly produced items could not be handled correctly if the computational resources of the processor became exhausted.
//!
//! This API however expresses infinite streams only, the producer cannot indicate that it has finished producing items and will never produce new ones. Thus the processor can never free associated resources. A different API which allows signaling the end of a datastream is the following:
//!
//! ```rust
//! trait Iterator {
//!     type Item;
//!
//!     fn produce(&mut self) -> Option<Self::Repeated>;
//! }
//! ```
//!
//! This solution describes finite sequences only, if it is used for an infinite sequence, then the type system would still enforce the presence of code that handles the case for when the sequence ends, even if it is known to the programmer that one is in fact dealing with an infinite sequence.
//!
//! Working with both the `Producer1` API and the `Iterator` API would lead to a lot of duplicate, almost identical code. This can be solved by adopting a more general view: both APIs make a distinction between the regular, arbitrarily often repeated items of the sequence, and the unique last item.
//!
//! For the first API, the type of the last item is the empty type, for which no instantiating value exists, thus expressing that this case never arises. For the second API, the type of the last item is the unit type, which conveys no information except that the end of the sequence has been reached. It now becomes straightforward to define an API which is able to express both scenarios:
//!
//! ```rust
//! pub enum SequenceState<R, L> {
//!     // More sequence items may follow.
//!     More(R),
//!     // No more sequence items will follow.
//!     Final(L),
//! }
//!
//! trait Producer2 {
//!     type Repeated;
//!     type Last;
//!
//!     fn produce(&mut self) -> SequenceState<Self::Repeated, Self::Last>;
//! }
//! ```
//!
//! The `Last` type can be meaningfully instantiated with more types than just the empty or the unit type. The sequence could for example consist of a number of operants followed by a single operator. Another typical example are fatal errors. If both a "regular"/"successful" end of the sequence or an erroneous end are possible, a result can be used. Non-fatal errors can be expressed by using a result for the `Repeated` type.
//!
//! A particularly nice consequence of this API design: the `Last` type can be instantiated by another sequence. This expresses the common concept of some sequence being followed by another one. For example, a communication session that begins with a key exchange followed by application-specific communication could be expressed as a sequence whose `Repeated` items are key exchange messages and whose `Last` type is the sequence of application-specific messages.
//!
//! Since the sequence ends after the `Last` item, `produce` may not be called after the last item has been received. Producers are allowed to to exhibit unspecified behavior if `produce` is called after they have emitted their last item.
//!
//! ### Duality
//!
//! This API allows a software component to receive data from a producer. Now we also need an API for the software component to send data to a consumer. Striving to keep this API dual to the producer API, we obtain the following by simply flipping the argument and return value of the `produce` method:
//!
//! ```rust
//! trait Consumer1 {
//!     type Repeated;
//!     type Last;
//!
//!     fn consume(&mut self, item: SequenceState<Self::Repeated, Self::Last>) -> ();
//! }
//! ```
//!
//! The associated contract is that `consume` may be called at most once with a `SequenceState::Last`.
//!
//! Typical consumers also need to be able to deal with the fact that item consumption may be fallible, e.g. a TCP socket might want to report back to the application that the connection has been lost. To that end we introduce a third associated type into the trait:
//!
//! ```rust
//! trait Consumer2 {
//!     type Repeated;
//!     type Last;
//!     type Stopped;
//!
//!     fn consume(&mut self, item: SequenceState<Self::Repeated, Self::Last>) -> Option<Self::Stopped>;
//! }
//! ```
//!
//! The semantics are that of an unrecoverable error, `consume` may not be called again after it returned `Some(x)`. Another interpretation is that the system receiving the data that is put into the consumer has stopped accepting more data. Maintaining the duality of producers and consumers, we update the producer API accordingly:
//!
//! ```rust
//! trait Producer3 {
//!     type Repeated;
//!     type Last;
//!     type Stopped;
//!
//!     fn produce(&mut self) -> SequenceState<Self::Repeated, Self::Last>;
//!     fn stop(&mut self, reason: Stopped);
//! }
//! ```
//!
//! After calling `stop`, no further method calls may be performed on a producer. `stop` does not return anything, since the producer will not be used anymore, whether the reason was successfully relayed to the data source or not.
//!
//! To maintain further symmetry, we can also transform the consumer API into a more traditional form of two separate methods for sending repeated items and for closing the consumer:
//!
//! ```rust
//! trait Consumer3 {
//!     type Repeated;
//!     type Last;
//!     type Stopped;
//!
//!     fn consume(&mut self, item: Self::Repeated) -> Option<Self::Stopped>;
//!     fn close(&mut self, item: Self::Last) -> Option<Self::Stopped>;
//! }
//! ```
//!
//! ### Buffering
//!
//! These APIs work on individual items, but there are many situations where operating on multiple items simultaneously is more efficient. Take for example a consumer of bytes that writes the received bytes into a file. A naive implementation of this requires a system call for every invocation of `consume`, which is very expensive. The system call however is designed to be able to write multiple bytes at once. We introduce the concept of buffering to enable consumers and producers to take advantage of such facilities.
//!
//! A buffering consumer doesn't immediately process items passed to `consume`, it merely promises to do so in the future. If it does not immediately process an item, it must remember it, so it is placed in a buffer. If that buffer ever gets full, as many items as possible are flushed from it, i.e., they are being processed simultaneously. This way, although the consumer presents an API that consumes individual items at a time, it operates internally on larger slices.
//!
//! The `close` method must first process all buffered items before closing the consumer. In addition to the automatic flushes when the buffer is full or the consumer is closed, the API must also provide a method for manually triggering processing of all currently buffered items:
//!
//! ```rust
//! trait Consumer {
//!     type Repeated;
//!     type Last;
//!     type Stopped;
//!
//!     fn consume(&mut self, item: Self::Repeated) -> Option<Self::Stopped>;
//!     fn close(&mut self, item: Self::Last) -> Option<Self::Stopped>;
//!     fn flush(&mut self) -> Option<Self::Stopped>;
//! }
//! ```
//!
//! The dual concept for producers is that of optimistic prefetching of data. Consider a producer of bytes that reads the bytes from a file. Performing a system call for every invocation of `produce` is again very inefficient. A buffering producer can instead read many bytes from the file at once, placing them in a buffer, and serving calls to `produce` from this buffer. Whenever `produce` is called but the buffer is empty, a new slice of bytes is fetched.
//!
//! The corresponding manual action is to slurp more items into the buffer even though it is not yet empty:
//!
//! ```rust
//! trait Producer {
//!     type Repeated;
//!     type Last;
//!     type Stopped;
//!
//!     fn produce(&mut self) -> SequenceState<Self::Repeated, Self::Last>;
//!     fn stop(&mut self, reason: Self::Stopped);
//!     fn slurp(&mut self);
//! }
//! ```
//!
//! This wraps up the design of the consumer and producer APIs for working with sequences of arbitrary length one item at a time.
//!
//! ## BulkProducers and BulkConsumers
//!
//! As already discussed in the section on buffering, it is often more efficient to work on multiple items at a time rather than individual items. A natural question is thus how to extend the consumer and producer APIs to allow consumption or production of multiple items at once.
//!
//! Since a producer/consumer that can operate on multiple items at a time is strictly more powerful than one that is restricted to operating on exactly one item at a time, it makes sense to define bulk producers/consumers via subtraits of regular producers/consumers.
//!
//! A first attempt at a bulk producer API could look as follows:
//!
//! ```rust
//! trait BulkProducer1: Producer {
//!     fn bulk_produce(&mut self, buffer: &mut[Self::Repeated]) -> SequenceState<usize, Self::Last>;
//! }
//! ```
//!
//! A call to `bulk_produce` is supplied with a buffer into which the producer writes data, returning how much data has been written. It must either write some number of items, or return the single `Last` item.
//!
//! It turns out that there are a number of issues with this approach, some more subtle than others. A problem that might seem rather contrived at first is that bulk producers are supposed to be specializations of regular producers, i.e. it should always be possible to express the behavior of `bulk_produce` in terms of multiple calls to `produce`. However, a type could provide the trivial `bulk_produce` implementation that always returns `Repeated(0)`, without needing to be a regular producer at all.
//!
//! While this is more of a theoretical wart than an actual problem, it points to another design issue: returning `Repeated(0)` never conveys actual information. If the `buffer` argument is a slice of length zero, then the method has to return `Repeated(0)` regardless of any specifics of the producer. And if the `buffer` the document has nonzero length, the return value should never be `Repeated(0)`: if no data will ever be produced again, the return value should be `Last(something)`, and if no data is available yet but it might be in the future, the function should block (or suspend execution and yield in the asynchronous case).
//!
//! Allowing the input buffer to have length zero and allowing to return zero as the number of produced items is thus completely redundant, but yet all implementing types have to ensure that they return `Repeated(0)` when presented with an empty input buffer. To fix this, let [`NonZeroUsize`](https://doc.rust-lang.org/std/num/struct.NonZeroUsize.html) be the type of all `usize` values, and let [`Slice1<T>`](https://docs.rs/slice_n/0.0.2/slice_n/type.Slice1.html) be the type of all nonempty slices containing items of type `T`. Then the following API removes the redundant case and incidentally prevents all semantically correct implementations of `bulk_produce` that cannot be expressed in terms of multiple `produce` usages:
//!
//! ```rust
//! trait BulkProducer2: Producer {
//!     fn bulk_produce(&mut self, buffer: &mut Slice1<Self::Repeated>) -> SequenceState<NonZeroUsize, Self::Last>;
//! }
//! ```
//!
//! The next issue is one of efficiency as well as API clarity. The intention behind `bulk_produce` is clearly that the producer can write data to the input `buffer`, but should not read data from it. The API however allows to do so. This has direct consequences for efficiency, because any buffer given to the method needs to contain initialized memory, as otherwise the producer might read uninitialized memory, which is undefined behavior. To make the API more explicit and to allow uninitialized memory, we can use a slice of [`MaybeUninit<Self::Repeated>`](https://doc.rust-lang.org/std/mem/union.MaybeUninit.html):
//!
//! ```rust
//! trait BulkProducer3: Producer {
//!     fn bulk_produce(&mut self, buffer: &mut Slice1<MaybeUninit<Self::Repeated>>) -> SequenceState<usize, Self::Last>;
//! }
//! ```
//!
//! This change results however in a problematic dilemma: the producer implementation does not know whether any given slot in the input `buffer` contains initialized memory or not. If it assumes the inputs to be uninitialized, it cannot drop the memory, so no destructors are run. While this is technically allowed in safe rust code, leaking memory is obviously not desired. If it however assumes the memory to be initialized and runs destructors, this is immediate undefined behavior if the memory is actually uninitialized.
//!
//! The solution is to restrict the `Repeated` type to those types for which not running destructors cannot violate any invariants: these are exactly the types that implement [`Copy`](https://doc.rust-lang.org/std/marker/trait.Copy.html). Since realistically speaking most bulk producers operate on `u8`s, this "restriction" is still a significant generalization over simply specializing the trait to `u8`s.
//!
//! ```rust
//! trait BulkProducer4: Producer where Self::Repeated: Copy {
//!     fn bulk_produce(&mut self, buffer: &mut Slice1<MaybeUninit<Self::Repeated>>) -> SequenceState<usize, Self::Last>;
//! }
//! ```
//!
//! To appreciate the next design problem with this API, we first need to construct the corresponding bulk consumer. Whereas the bulk producer writes items into a mutable buffer, a bulk consumer reads them from an immutable buffer and returns how many have been read:
//!
//! ```rust
//! trait BulkConsumer1: Consumer {
//!     fn bulk_consume(&mut self, buffer: &[Self::Repeated]) -> SequenceState<usize, Self::Stopped>;
//! }
//! ```
//!
//! This API has the same problem with zero length buffers, so we update it to use more accurate types:
//!
//! ```rust
//! trait BulkConsumer2: Consumer {
//!     fn bulk_consume(&mut self, buffer: &Slice1<Self::Repeated>) -> SequenceState<NonZeroUsize, Self::Stopped>;
//! }
//! ```
//!
//! While all memory in the input `buffer` must be initialized, we nevertheless restrict the `Repeated` type to implement `Copy` - for now simply to maintain duality, but we will shortly develop modifications to the API which require the bound anyways.
//!
//! ```rust
//! trait BulkConsumer4: Consumer where Self::Repeated: Copy {
//!     fn bulk_consume(&mut self, buffer: &Slice1<Self::Repeated>) -> SequenceState<NonZeroUsize, Self::Stopped>;
//! }
//! ```
//!
//! The remaining problem with these APIs lies in their interplay. What does a function look like that pipes all data from a bulk producer into a compatible bulk consumer by using `bulk_produce` and `bulk_consume`? It has to allocate an intermediate buffer into which the producer can write items and from which the consumer can read them. If the buffer is too small, the gains from processing data in bulk diminish. If the buffer is too large, memory is wasted. And since the function should be generic over all possible producers and consumers, it cannot get the buffer size right for every use case. And even if it could: the intermediate buffer simply should not be necessary, its existence is an artifact of the API design, not an inherent necessity for getting items from a data source to a data sink.
//!
//! We can alleviate this problem by letting the producer or consumer expose their internal buffer for direct access from the other component. The producer API for example can provide a method that returns a slice from which items can be read, as well as a method by which consuming code can tell the producer that it has consumed some number of items, so that the producer can accordingly advance the starting point of the next slice it exposes.
//!
//! ```rust
//! trait BulkProducer5: Producer where Self::Repeated: Copy {
//!     fn producer_slots(&self) -> SequenceState<&Slice1<Self::Repeated>, Self::Last>;
//!     fn did_produce(&mut self, amount: NonZeroUsize);
//!
//!     fn bulk_produce(&mut self, buffer: &mut Slice1<MaybeUninit<Self::Repeated>>) -> SequenceState<usize, Self::Last>;
//! }
//! ```
//!
//! With this API, one can write a function that pipes data from the producer to the consumer by passing the `producer_slots` as the `buffer` argument to `bulk_consume`.
//!
//! Similarly we can add methods to the bulk consumer API that expose its internal buffer. This is where the `Copy` requirement becomes necessary, it removes the requirement for the exposed buffer to contain initialized memory, since it should only be written to, not read from.
//!
//! ```rust
//! trait BulkConsumer5: Consumer where Self::Repeated: Copy {
//!     fn consumer_slots(&mut self) -> SequenceState<&mut Slice1<MaybeUninit<Self::Repeated>>, Self::Stopped>;
//!     unsafe fn did_consume(&mut self, amount: NonZeroUsize);
//!
//!     fn bulk_consume(&mut self, buffer: &Slice1<Self::Repeated>) -> SequenceState<NonZeroUsize, Self::Stopped>;
//! }
//! ```
//!
//! With this API it becomes possible to write function that pipes data from the producer to the consumer by passing the `consumer_slots` as the `buffer` argument to `bulk_producer`.
//!
//! Unfortunately `did_consume` has to be `unsafe`, so that the consumer is allowed to assume that the memory in its buffer has been initialized after a corresponding call to `did_consume`, which causes undefined behavior if `did_consume` was not invoked according to the contract of the API.
//!
//! As a final improvement, we can express [`bulk_consume`](crate::sync::bulk_consume) and [`bulk_produce`](crate::sync::bulk_produce) in terms of the other two API methods, providing them as functions and removing them from the traits.
//!
//! ## Summary
//!
//! To summarize, the `Producer` and `Consumer` APIs provide an abstraction for working with sequences one item at a time, allowing to deal with sequences of unknown, arbitrary length. The abstractions draw a distinction between those parts of a sequence that may be repeated, and those which indicate the end of the sequence or the end of the sequence processing. Buffering enables implementers of these abstractions to use efficient bulk-processing internally while still presenting a single-item interface.
//!
//! The `BulkProducer` and `BulkConsumer` APIs augment the regular `Producer` and `Consumer` APIs by additionally exposing a slice-oriented interface. They can work with slices into externally provided buffers, but they can also expose their internal buffers so that the requirement for external memory allocations can be sidestepped.
