[![Build Status](https://travis-ci.org/Xudong-Huang/may_actor.svg?branch=master)](https://travis-ci.org/Xudong-Huang/may_actor)
[![Build status](https://ci.appveyor.com/api/projects/status/7gv4kw3b0m0y1iy6/branch/master?svg=true)](https://ci.appveyor.com/project/Xudong-Huang/may_actor/branch
/master)
[![Current Crates.io Version](https://img.shields.io/crates/v/may_actor.svg)](https://crates.io/crates/may_actor)
[![Document](https://img.shields.io/badge/doc-may_actor-green.svg)](https://docs.rs/may_actor)


# may_actor

rust native actor library based on [may](https://github.com/Xudong-Huang/may)


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

## Notice

This simple library doesn't support spawn actors across processes

## License

`may_actor` is licensed under either of the following, at your option:

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
