//! # may_actor
//!
//! rust native actor library based on [may](https://github.com/Xudong-Huang/may)
//!
//! ## Features
//!
//! - run closure asynchronously by send message ([`Actor.call`])
//! - run closure synchronously with the actor's internal state ([`Actor.with`])
//! - convert from raw instance reference to Actor ([`Actor.from`])
//! - allow panic inside a closure message
//!
//! ## Notice
//!
//! This simple library doesn't support spawn actors across processes
//!
//! [`Actor.call`]: ./struct.Actor.html#method.call
//! [`Actor.with`]: ./struct.Actor.html#method.with
//! [`Actor.from`]: ./struct.Actor.html#method.from
//!
#![deny(missing_docs)]

#[doc(hidden)]
#[macro_use]
extern crate may;

#[doc(hidden)]
extern crate may_waiter;

use std::cell::UnsafeCell;
use std::panic::{self, RefUnwindSafe};
use std::sync::{Arc, Weak};

use may::sync::mpsc;
use may_waiter::Waiter;

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
}

/// coroutine based Actor.
///
/// The type `Actor<T>` wraps `T` into an Actor.
/// You can send message to the actor by calling it's [`call`] method.
/// You can run a closure synchronously with the actor internal state by calling it's [`with`] method.
///
/// # Examples
///
/// ```
/// use may_actor::Actor;
///
/// let a = Actor::new(40);
/// a.call(|me| *me += 2);
/// a.with(|me| assert_eq!(*me, 42));
/// ```
///
/// [`call`]: ./struct.Actor.html#method.call
/// [`with`]: ./struct.Actor.html#method.with
#[derive(Debug)]
pub struct Actor<T> {
    inner: Arc<ActorImpl<T>>,
}

unsafe impl<T> Send for Actor<T> {}

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

    /// create an actor with a driver coroutine running in backgroud
    /// when all actor instances got dropped, the driver coroutine
    /// would be cancelled
    pub fn drive_new<F>(data: T, f: F) -> Self
    where
        F: FnOnce(DriverActor<T>) + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = mpsc::channel::<Box<FnBox>>();

        let actor = Actor {
            inner: Arc::new(ActorImpl {
                data: UnsafeCell::new(data),
                tx: tx,
            }),
        };

        // create the back groud driver
        let driver_para = Arc::downgrade(&actor.inner);
        let driver = go!(|| f(DriverActor { inner: driver_para }));

        // when all tx are dropped, the coroutine would exit
        go!(move || {
            for f in rx.into_iter() {
                panic::catch_unwind(panic::AssertUnwindSafe(move || {
                    f.call_box();
                })).ok();
            }
            // when all the actor instances dropped, cancel the driver
            unsafe { driver.coroutine().cancel() };
            driver.join().ok();
            println!("actor coroutine done");
        });

        actor
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

    /// execute a closure in the actor's coroutine context
    /// and wait for the result.
    ///
    /// This is a sync version of `call` method, it will
    /// block until finished, panic will be propogate to the
    /// caller's context.
    /// You can use this method to monitor the internal state
    pub fn with<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R + Send + 'static,
        T: Send + 'static,
        R: Send + 'static,
    {
        let waiter = Arc::new(Waiter::new());
        {
            let waiter = waiter.clone();
            let actor = self.inner.clone();
            let f = move || {
                let data = actor.get_mut();
                // signal the caller to after processing
                waiter.set_rsp(f(data))
            };

            self.inner.tx.send(Box::new(f)).unwrap();
        }

        // wait until the viewer pause the message processing
        waiter.wait_rsp(None).unwrap()
    }
}

/// parameter used in driver coroutine function
#[derive(Debug)]
pub struct DriverActor<T> {
    // hold a weak pointer
    // so that have chance to clean up
    inner: Weak<ActorImpl<T>>,
}

impl<T> DriverActor<T> {
    /// same as Actor.call
    pub fn call<F>(&self, f: F)
    where
        F: FnOnce(&mut T) + Send + 'static,
        T: Send + 'static,
    {
        // if the actor is gone, cancel the current coroutine
        let actor = match self.inner.upgrade() {
            None => may::coroutine::trigger_cancel_panic(),
            Some(inner) => Actor { inner },
        };
        actor.call(f);
    }

    /// same as Actor.with
    pub fn with<R, F>(&self, f: F)
    where
        F: FnOnce(&mut T) -> R + Send + 'static,
        T: Send + 'static,
        R: Send + 'static,
    {
        // if the actor is gone, cancel the current coroutine
        let actor = match self.inner.upgrade() {
            None => may::coroutine::trigger_cancel_panic(),
            Some(inner) => Actor { inner },
        };
        actor.with(f);
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
        // the with mothod would wait previous messages process done
        a.with(|me| assert_eq!(*me, 6));
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

        ping.with(|me| assert_eq!(me.count, 11));
        pong.with(|me| assert_eq!(me.count, 11));
    }

    #[test]
    fn driver_actor() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::Duration;

        #[derive(Debug)]
        struct DropFlag {
            flag: Arc<AtomicBool>,
        }

        impl Drop for DropFlag {
            fn drop(&mut self) {
                self.flag.store(true, Ordering::Relaxed);
            }
        }

        let flag = Arc::new(AtomicBool::new(false));

        let drop_flag = DropFlag { flag: flag.clone() };

        let actor = Actor::drive_new(0, move |me| loop {
            me.call(|v| {
                *v += 1;
                println!("new_value = {}", *v)
            });
            assert_eq!(drop_flag.flag.load(Ordering::Relaxed), false);
            may::coroutine::sleep(Duration::from_secs(1));
        });

        may::coroutine::sleep(Duration::from_secs(3));
        drop(actor);
        // wait some time for the driver coroutine exit
        may::coroutine::sleep(Duration::from_millis(100));
        assert_eq!(flag.load(Ordering::Relaxed), true);
    }
}
