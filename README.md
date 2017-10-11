# may_acotr

rust native actor library based on [may]()


## Usage
```rust
extern crate may_actor;
use may_actor::Actor;

fn main() {
    struct HelloActor(u32);
    let a = Actor::new(HelloActor(0));

    a.call(|me| {
        me.0 = 10;
        println!("hello world");
    });
    // the view would wait previous messages process done
    a.view(|me| println!("actor value is {}", me.0));
}
```

for a detailed example, please see [pi.rs](examples/pi.rs)

## Features

- [x] send message via closure (`Actor.call()`)
- [x] view internal actor state via closure (`Actor.view()`)
- [x] convert from raw instance reference to Actor (`Actor.from()`)
- [x] allow panic inside a closure message
