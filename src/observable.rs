use crate::prelude::*;

mod from;
pub use from::*;
pub(crate) mod from_future;
pub use from_future::{from_future, from_future_with_err};
pub(crate) mod interval;
pub use interval::{interval, interval_at};

/// A representation of any set of values over any amount of time. This is the
/// most basic building block rxrust
///
#[derive(Clone)]

pub struct Observable<F>(F);

impl<F> Observable<F> {
  /// param `subscribe`: the function that is called when the Observable is
  /// initially subscribed to. This function is given a Subscriber, to which
  /// new values can be `next`ed, or an `error` method can be called to raise
  /// an error, or `complete` can be called to notify of a successful
  /// completion.
  pub fn new<S, U>(subscribe: F) -> Self
  where
    F: FnOnce(Subscriber<S, U>),
  {
    Self(subscribe)
  }
}

impl<F> Fork for Observable<F>
where
  F: Clone,
{
  type Output = ForkObservable<Observable<F>>;
  #[inline(always)]
  fn fork(&self) -> Self::Output { ForkObservable(self.clone()) }
}

impl<F, Item, Err, S, U> RawSubscribable<Item, Err, Subscriber<S, U>>
  for Observable<F>
where
  S: Observer<Item, Err>,
  F: FnOnce(Subscriber<S, U>),
  U: SubscriptionLike + Clone + 'static,
{
  type Unsub = U;
  fn raw_subscribe(self, subscriber: Subscriber<S, U>) -> Self::Unsub {
    let subscription = subscriber.subscription.clone();
    (self.0)(subscriber);
    subscription
  }
}

#[derive(Clone)]
pub struct SharedObservable<F>(F);

impl<O> IntoShared for SharedObservable<O>
where
  Self: Send + Sync + 'static,
{
  type Shared = Self;
  #[inline(always)]
  fn to_shared(self) -> Self::Shared { self }
}

impl<O> Fork for SharedObservable<O>
where
  O: Clone,
{
  type Output = ForkObservable<SharedObservable<O>>;
  #[inline(always)]
  fn fork(&self) -> Self::Output { ForkObservable(self.clone()) }
}

impl<O, Item, Err, S, U> RawSubscribable<Item, Err, Subscriber<S, U>>
  for SharedObservable<O>
where
  S: Observer<Item, Err> + IntoShared,
  O: RawSubscribable<Item, Err, Subscriber<S::Shared, U::Shared>>,
  U: IntoShared<Shared = SharedSubscription> + SubscriptionLike,
{
  type Unsub = U::Shared;
  fn raw_subscribe(self, subscriber: Subscriber<S, U>) -> Self::Unsub {
    let subscriber = subscriber.to_shared();
    let subscription = subscriber.subscription.clone();
    self.0.raw_subscribe(subscriber);
    subscription
  }
}

impl<F> IntoShared for Observable<F>
where
  F: Send + Sync + 'static,
{
  type Shared = SharedObservable<Observable<F>>;
  fn to_shared(self) -> Self::Shared { SharedObservable(self) }
}

#[derive(Clone)]
pub struct ForkObservable<O>(O);

impl<F> IntoShared for ForkObservable<F>
where
  F: Send + Sync + 'static,
{
  type Shared = SharedObservable<ForkObservable<F>>;
  fn to_shared(self) -> Self::Shared { SharedObservable(self) }
}

impl<O> Fork for ForkObservable<O>
where
  O: Clone,
{
  type Output = ForkObservable<O>;
  #[inline(always)]
  fn fork(&self) -> Self::Output { self.clone() }
}

impl<'a, F, Item, Err, S, U> RawSubscribable<Item, Err, Subscriber<S, U>>
  for ForkObservable<Observable<F>>
where
  S: Observer<Item, Err> + 'a,
  F: FnOnce(Subscriber<Box<dyn Observer<Item, Err> + 'a>, U>),
  U: SubscriptionLike + Clone + 'static,
{
  type Unsub = U;
  fn raw_subscribe(self, subscriber: Subscriber<S, U>) -> Self::Unsub {
    let observer: Box<dyn Observer<Item, Err> + 'a> =
      Box::new(subscriber.observer);
    let subscriber = Subscriber {
      observer,
      subscription: subscriber.subscription,
    };

    let subscription = subscriber.subscription.clone();
    ((self.0).0)(subscriber);
    subscription
  }
}

impl<O, Item, Err, S, U> RawSubscribable<Item, Err, Subscriber<S, U>>
  for ForkObservable<SharedObservable<O>>
where
  Item: 'static,
  Err: 'static,
  S: Observer<Item, Err> + Send + Sync + 'static,
  O: RawSubscribable<
    Item,
    Err,
    Subscriber<Box<dyn Observer<Item, Err> + Send + Sync>, SharedSubscription>,
  >,
  U: SubscriptionLike + IntoShared<Shared = SharedSubscription>,
{
  type Unsub = SharedSubscription;
  fn raw_subscribe(self, subscriber: Subscriber<S, U>) -> Self::Unsub {
    let observer: Box<dyn Observer<Item, Err> + Send + Sync> =
      Box::new(subscriber.observer);
    let subscription = subscriber.subscription.to_shared();
    self.0.raw_subscribe(Subscriber {
      observer,
      subscription: subscription.clone(),
    });
    subscription
  }
}

#[cfg(test)]
mod test {
  use crate::ops::Fork;
  use crate::prelude::*;
  use std::sync::{Arc, Mutex};

  #[test]
  fn proxy_call() {
    let next = Arc::new(Mutex::new(0));
    let err = Arc::new(Mutex::new(0));
    let complete = Arc::new(Mutex::new(0));
    let c_next = next.clone();
    let c_err = err.clone();
    let c_complete = complete.clone();

    Observable::new(|mut subscriber| {
      subscriber.next(&1);
      subscriber.next(&2);
      subscriber.next(&3);
      subscriber.complete();
      subscriber.next(&3);
      subscriber.error(&"never dispatch error");
    })
    .to_shared()
    .subscribe_all(
      move |_| *next.lock().unwrap() += 1,
      move |_: &&str| *err.lock().unwrap() += 1,
      move || *complete.lock().unwrap() += 1,
    );

    assert_eq!(*c_next.lock().unwrap(), 3);
    assert_eq!(*c_complete.lock().unwrap(), 1);
    assert_eq!(*c_err.lock().unwrap(), 0);
  }
  #[test]
  fn support_fork() {
    let o = Observable::new(|mut subscriber| {
      subscriber.next(&1);
      subscriber.next(&2);
      subscriber.next(&3);
      subscriber.next(&4);
    });
    let sum1 = Arc::new(Mutex::new(0));
    let sum2 = Arc::new(Mutex::new(0));
    let c_sum1 = sum1.clone();
    let c_sum2 = sum2.clone();
    o.fork().subscribe(move |v| *sum1.lock().unwrap() += v);
    o.fork().subscribe(move |v| *sum2.lock().unwrap() += v);

    assert_eq!(*c_sum1.lock().unwrap(), 10);
    assert_eq!(*c_sum2.lock().unwrap(), 10);
  }

  #[test]
  fn fork_and_share() {
    let observable = observable::empty!();
    // shared after fork
    observable.fork().to_shared().subscribe(|_: &()| {});
    observable.fork().to_shared().subscribe(|_| {});

    // shared before fork
    let observable = observable::empty!().to_shared();
    observable.fork().subscribe(|_: &()| {});
    observable.fork().subscribe(|_| {});
  }
}
