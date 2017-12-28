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
        // need to transfer to the raw trait object so that we don't need the UnwindSafe bound
        // we can use std::raw::TraitObject, but it is nightly only
        #[repr(C)]
        struct TraitObject {
            pub data: *mut (),
            pub vtable: *mut (),
        }

        let (tx, rx) = mpsc::channel::<Box<FnBox>>();
        // when all tx are dropped, the coroutine would exit
        go!(move || for f in rx.into_iter() {
            // ignore the panic if it happened
            let f: TraitObject = unsafe { std::mem::transmute(Box::into_raw(f)) };
            panic::catch_unwind(move || {
                let f: *mut FnBox = unsafe { std::mem::transmute(f) };
                let f = unsafe { Box::from_raw(f) };
                f.call_box();
            }).ok();
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

#[derive(Debug)]
pub struct Actor<T> {
    inner: Arc<ActorImpl<T>>,
}

impl<T> Clone for Actor<T> {
    fn clone(&self) -> Self {
        Actor { inner: self.inner.clone() }
    }
}

impl<T> Actor<T> {
    pub fn new(actor: T) -> Self {
        Actor { inner: Arc::new(ActorImpl::new(actor)) }
    }

    /// convert from innter ref to actor
    /// only valid if &T is coming from an actor
    pub unsafe fn from(inner: &T) -> Self {
        // how to find the outer wrapper?
        let m: *const ActorImpl<T> = (inner as *const _ as usize) as *const _;
        let arc = Arc::from_raw(m);
        let ret = Actor { inner: arc.clone() };
        std::mem::forget(arc);
        ret
    }

    /// send to the actor a 'message' by manipulating the actor
    /// the raw actor type must be Send and 'static
    /// so that it can be used by multi threads
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

    /// view the actor internel states
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
