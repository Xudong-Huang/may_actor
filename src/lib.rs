//! # may_actor
//!
//! rust native actor library based on [may](https://github.com/Xudong-Huang/may)
//!
//! ## Features
//!
//! - send message via closure ([`Actor.call`])
//! - view internal actor state via closure ([`Actor.view`])
//! - convert from raw instance reference to Actor ([`Actor.from`])
//! - allow panic inside a closure message
//!
//! ## Notice
//!
//! This simple library doesn't support spawn actors across processes
//!
//! [`Actor.call`]: ./struct.Actor.html#method.call
//! [`Actor.view`]: ./struct.Actor.html#method.view
//! [`Actor.from`]: ./struct.Actor.html#method.from
//!
#![deny(missing_docs)]

#[doc(hidden)]
#[macro_use]
extern crate may;

use std::sync::Arc;
use std::cell::UnsafeCell;
use std::panic::{self, RefUnwindSafe};
use may::sync::mpsc;

#[doc(hidden)]
trait FnBox: Send {
    fn call_box(self: Box<Self>);
}

impl<F: FnOnce() + Send> FnBox for F {
    fn call_box(self: Box<Self>) {
        (*self)()
    }
}

// we must use repr(C) to fix the mem layout
#[repr(C)]
#[derive(Debug)]
struct ActorImpl<T> {
    data: UnsafeCell<T>,
    tx: mpsc::Sender<Box<FnBox>>,
}

unsafe impl<T: Send> Sync for ActorImpl<T> {}
impl<T> RefUnwindSafe for ActorImpl<T> {}

impl<T> ActorImpl<T> {
    fn new(data: T) -> Self {
        let (tx, rx) = mpsc::channel::<Box<FnBox>>();
        // when all tx are dropped, the coroutine would exit
        go!(move || for f in rx.into_iter() {
            panic::catch_unwind(panic::AssertUnwindSafe(move || {
                f.call_box();
            })).ok();
        });

        ActorImpl {
            data: UnsafeCell::new(data),
            tx: tx,
        }
    }

    fn get_mut(&self) -> &mut T {
        unsafe { &mut *self.data.get() }
    }

    fn get_ref(&self) -> &T {
        unsafe { &*self.data.get() }
    }
}

/// coroutine based Actor.
///
/// The type `Actor<T>` wraps `T` into an Actor.
/// You can send messages to the actor by calling it's [`call`] method.
/// You can view the actor internal state by calling it's [`view`] method.
///
/// # Examples
///
/// ```
/// use may_actor::Actor;
///
/// let a = Actor::new(40);
/// a.call(|me| *me += 2);
/// a.view(|me| assert_eq!(*me, 42));
/// ```
///
/// [`call`]: ./struct.Actor.html#method.call
/// [`view`]: ./struct.Actor.html#method.view
#[derive(Debug)]
pub struct Actor<T> {
    inner: Arc<ActorImpl<T>>,
}

impl<T> Clone for Actor<T> {
    fn clone(&self) -> Self {
        Actor {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Actor<T> {
    /// create an actor by consuming the actual actor implementation
    pub fn new(actor: T) -> Self {
        Actor {
            inner: Arc::new(ActorImpl::new(actor)),
        }
    }

    /// convert from inner ref to actor
    ///
    /// ## Safety
    /// only valid if `&T`is coming from an actor.
    /// normally this is used to convert `&self` to `Actor<T>`
    pub unsafe fn from(inner: &T) -> Self {
        // how to find the outer wrapper?
        let m: *const ActorImpl<T> = (inner as *const _ as usize) as *const _;
        let arc = Arc::from_raw(m);
        let ret = Actor { inner: arc.clone() };
        std::mem::forget(arc);
        ret
    }

    /// send the actor a 'message' by a closure.
    ///
    /// the closure would get the `&mut T` as parameter,
    /// so that you can manipulate its internal state.
    ///
    /// the raw actor type must be `Send` and `'static`
    /// so that it can be used by multi threads.
    ///
    /// the closure would be executed asynchronously
    pub fn call<F>(&self, f: F)
    where
        F: FnOnce(&mut T) + Send + 'static,
        T: Send + 'static,
    {
        let actor = self.inner.clone();
        let f = move || {
            let data = actor.get_mut();
            f(data);
        };

        self.inner.tx.send(Box::new(f)).unwrap();
    }

    /// view the actor internal states by a closure.
    ///
    /// the closure would get a `&T` as parameter,
    /// so that you can access its internal state but not change it.
    ///
    /// this function would block until the view done
    pub fn view<F>(&self, f: F)
    where
        F: FnOnce(&T),
    {
        use may::sync::Blocker;

        let blocker = Blocker::current();
        let (tx, rx) = mpsc::channel();

        {
            let blocker = blocker.clone();

            // this is only a blocker that hold the message processing
            let viewer = move || {
                // signal the viewer to start processing
                blocker.unpark();
                // wait until the viewer processing done
                let _ = rx.recv().unwrap();
            };

            self.inner.tx.send(Box::new(viewer)).unwrap();
        }

        // wait until the viewer pause the message processing
        blocker.park(None).unwrap();

        let data = self.inner.get_ref();
        f(data);

        // after view the data, unblock the message processing
        tx.send(()).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let i = 0u32;
        let a = Actor::new(i);
        a.call(|me| *me += 2);
        a.call(|_me| panic!("support panic inside"));
        a.call(|me| *me += 4);
        // the view would wait previous messages process done
        a.view(|me| assert_eq!(*me, 6));
    }

    #[test]
    fn ping_pong() {
        struct Ping {
            count: u32,
            tx: mpsc::Sender<()>,
        };

        struct Pong {
            count: u32,
        };

        impl Ping {
            fn ping(&mut self, to: Actor<Pong>) {
                if self.count > 10 {
                    self.tx.send(()).unwrap();
                    return;
                }

                println!("ping called, count={}", self.count);
                self.count += 1;
                let ping = unsafe { Actor::from(self) };
                to.call(move |pong| pong.pong(ping));
            }
        }

        impl Pong {
            fn pong(&mut self, to: Actor<Ping>) {
                println!("pong called, count={}", self.count);
                self.count += 1;
                let pong = unsafe { Actor::from(self) };
                to.call(|ping| ping.ping(pong))
            }
        }

        let (tx, rx) = mpsc::channel();

        let ping = Actor::new(Ping { count: 0, tx: tx });
        let pong = Actor::new(Pong { count: 0 });

        {
            let pong = pong.clone();
            ping.call(|me| me.ping(pong));
        }

        // wait the message process finish
        rx.recv().unwrap();

        ping.view(|me| assert_eq!(me.count, 11));
        pong.view(|me| assert_eq!(me.count, 11));
    }
}
