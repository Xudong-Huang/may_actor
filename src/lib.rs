extern crate may;
#[macro_use]
extern crate lazy_static;

use std::sync::Arc;
use may::coroutine;
use may::sync::{Mutex, mpmc};

#[doc(hidden)]
trait FnBox: Send {
    fn call_box(self: Box<Self>);
}

impl<F: FnOnce() + Send> FnBox for F {
    fn call_box(self: Box<Self>) {
        (*self)()
    }
}

#[derive(Debug)]
pub struct ActorRunner {
    tx: mpmc::Sender<Box<FnBox>>,
}

impl ActorRunner {
    fn new(workers: u32) -> Self {
        let (tx, rx) = mpmc::channel::<Box<FnBox>>();
        for _ in 0..workers {
            let rx = rx.clone();
            coroutine::spawn(move || for f in rx.into_iter() {
                f.call_box();
            });
        }

        ActorRunner { tx: tx }
    }

    pub fn add<F: FnOnce() + Send + 'static>(&self, f: F) {
        self.tx.send(Box::new(f)).unwrap();
    }
}

lazy_static! {
    pub static ref ACTOR_RUNNER: ActorRunner = ActorRunner::new(100);
}

#[derive(Debug)]
pub struct Actor<T> {
    raw: Arc<Mutex<T>>,
}

impl<T> Clone for Actor<T> {
    fn clone(&self) -> Self {
        Actor { raw: self.raw.clone() }
    }
}

impl<T> Actor<T> {
    /// calc the offset of inner data and Actor
    fn offset() -> usize {
        use std::ops::Deref;

        // TODO: how to forget this invalid data to prevent the drop called?
        let data: T = unsafe { ::std::mem::zeroed() };
        let invalid = Actor::new(data);
        let offset = {
            let g = invalid.raw.lock().unwrap();
            (g.deref() as *const T as usize) - (invalid.raw.deref() as *const Mutex<T> as usize)
        };
        offset
    }

    pub fn new(actor: T) -> Self {
        Actor { raw: Arc::new(Mutex::new(actor)) }
    }

    /// convert from innter ref to actor
    /// only valid if &T is coming from an actor
    pub unsafe fn from(inner: &T) -> Self {
        // how to find the outer wrapper?
        let m: *const Mutex<T> = ((inner as *const _ as usize) - Self::offset()) as *const _;
        let arc = Arc::from_raw(m);
        let ret = Actor { raw: arc.clone() };
        ::std::mem::forget(arc);
        ret
    }

    /// send to the actor a 'message' by manipulating the actor
    /// the raw actor type must be Send and 'static
    /// so that it can be used by multi thread
    /// if the closure blocks, the worker coroutine would be suspended
    /// and would consume all the worker coroutines so there would need
    /// more coroutines to process the message
    pub fn call<F>(&self, f: F)
    where
        F: FnOnce(&mut T) + Send + 'static,
        T: Send + 'static,
    {
        let actor = self.raw.clone();
        let f = move || {
            let mut g = actor.lock().unwrap();
            f(&mut g);
        };

        // TODO: expose the intenal queue size for the tx/rx for mpmc channel
        let pending = 0;

        // if there are too many actor messages need to process which means the worker
        // coroutines are blcoked by the actor message processing internally
        // don't use the runner, create a coroutine directly to process the message
        if pending > 100 {
            coroutine::spawn(f);
        } else {
            ACTOR_RUNNER.add(f);
        }
    }

    /// view the actor internel states
    pub fn view<F>(&self, f: F)
    where
        F: FnOnce(&T),
    {
        let g = self.raw.lock().unwrap();
        f(&g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {

        let i = 0u32;
        let a = Actor::new(i);
        a.call(|me| { *me += 2; });
        a.call(|me| { *me += 4; });
        // sleep a while to let the actor process messages
        coroutine::sleep(::std::time::Duration::from_millis(10));

        a.view(|me| assert_eq!(*me, 6));
    }

    #[test]
    fn ping_pong() {
        struct Ping {
            count: u32,
        };
        struct Pong {
            count: u32,
        };

        impl Ping {
            fn ping(&mut self, to: Actor<Pong>) {
                if self.count > 10 {
                    return;
                }

                println!("ping called");
                self.count += 1;
                let ping = unsafe { Actor::from(self) };
                to.call(move |pong| { pong.pong(ping); });
            }
        }

        impl Pong {
            fn pong(&mut self, to: Actor<Ping>) {
                println!("pong called");
                self.count += 1;
                let pong = unsafe { Actor::from(self) };
                to.call(move |ping| { ping.ping(pong); })
            }
        }

        let ping = Actor::new(Ping { count: 0 });
        let pong = Actor::new(Pong { count: 0 });

        {
            let pong = pong.clone();
            ping.call(move |me| { me.ping(pong); });
        }

        coroutine::sleep(::std::time::Duration::from_secs(1));

        ping.view(|me| assert_eq!(me.count, 11));
        pong.view(|me| assert_eq!(me.count, 11));
    }
}
