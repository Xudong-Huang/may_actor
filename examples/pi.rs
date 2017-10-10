extern crate may;
extern crate may_actor;

use may::coroutine;
use may::sync::mpsc;
use may_actor::Actor;

const TOTAL_NUM: usize = 100000000;
const WORK_LOAD: usize = 10000;
const ACTOR_NUMBER: usize = 100;

fn pow_of_minus_one(pow: usize) -> f64 {
    if pow & 1 != 0 { -1.0 } else { 1.0 }
}

fn calc_work(start: usize, end: usize) -> f64 {
    let mut sum = 0.0;
    for i in start..end {
        sum += pow_of_minus_one(i) / (2.0 * i as f64 + 1.0)
    }
    sum * 4.0
}

// single thread version for pi
fn pi() -> f64 {
    calc_work(0, TOTAL_NUM)
}

// coroutine version for pi
fn pi_coroutine() -> f64 {
    let works = TOTAL_NUM / WORK_LOAD;
    let (tx, rx) = mpsc::channel();

    let mut start = 0;
    for _ in 0..works {
        let tx = tx.clone();
        let end = start + WORK_LOAD;
        coroutine::spawn(move || tx.send(calc_work(start, end)).unwrap());
        start = end;
    }

    let mut sum = 0.0;
    for _ in 0..works {
        sum += rx.recv().unwrap();
    }
    sum
}

// actor version for pi
fn pi_actor() -> f64 {
    const WORKS: usize = TOTAL_NUM / WORK_LOAD;

    struct Master {
        pi: f64,
        count: usize,
        tx: mpsc::Sender<f64>,
    }

    impl Master {
        fn recv_data(&mut self, data: f64) {
            self.pi += data;
            self.count += 1;
            if self.count == WORKS {
                self.tx.send(self.pi).unwrap();
            }
        }
    }

    struct Worker;

    impl Worker {
        fn work(&self, master: Actor<Master>, start: usize, end: usize) {
            let data = calc_work(start, end);
            master.call(move |me| me.recv_data(data));
        }
    }

    let (tx, rx) = mpsc::channel();

    // create the master actor
    let master = Actor::new(Master {
        pi: 0.0,
        count: 0,
        tx: tx,
    });

    // create the worker actors
    let mut workers = vec![];
    for _ in 0..ACTOR_NUMBER {
        workers.push(Actor::new(Worker));
    }

    // send work load to workers
    let mut start = 0;
    let mut idx = 0;
    for _ in 0..WORKS {
        let end = start + WORK_LOAD;
        let master = master.clone();
        workers[idx].call(move |me| me.work(master, start, end));
        start = end;
        idx += 1;
        if idx == ACTOR_NUMBER {
            idx = 0;
        }
    }

    rx.recv().unwrap()
}

fn main() {
    may::config().set_workers(4);
    let dur = std::time::Instant::now();
    println!("pi is {}, dur = {:?}", pi(), dur.elapsed());
    let dur = std::time::Instant::now();
    println!("pi is {}, dur = {:?}", pi_coroutine(), dur.elapsed());
    let dur = std::time::Instant::now();
    println!("pi is {}, dur = {:?}", pi_actor(), dur.elapsed());
}
