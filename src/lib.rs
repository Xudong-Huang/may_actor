extern crate may;

use std::sync::Arc;
use may::coroutine;
use may::sync::Mutex;

#[derive(Debug)]
pub struct Actor<T> {
    raw: Arc<Mutex<T>>,
}

impl<T> Actor<T> {
    pub fn new(actor: T) -> Self {
        Actor { raw: Arc::new(Mutex::new(actor)) }
    }

    pub fn call<F>(&self, f: F)
    where
        F: FnOnce(&mut T) + Send + 'static,
        T: Send + 'static,
    {
        let actor = self.raw.clone();
        coroutine::spawn(move || {
            let mut g = actor.lock().unwrap();
            f(&mut g);
        });
    }

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
        a.call(|me| {
            println!("a={}", me);
        });

        coroutine::sleep(::std::time::Duration::from_secs(1));

        a.view(|me| assert_eq!(*me, 6));
    }
}
