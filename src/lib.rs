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

impl<T> Actor<T> {
    pub fn new(actor: T) -> Self {
        Actor { raw: Arc::new(Mutex::new(actor)) }
    }

    /// send to the actor a 'message' by manipulating the actor
    /// the raw actor type must be Send and 'static
    /// so that it can be used by multi thread
    pub fn call<F>(&self, f: F)
    where
        F: FnOnce(&mut T) + Send + 'static,
        T: Send + 'static,
    {
        let actor = self.raw.clone();
        // coroutine::spawn(move || {
        ACTOR_RUNNER.add(move || {
            let mut g = actor.lock().unwrap();
            f(&mut g);
        });
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
}
