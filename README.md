[![Build Status](https://travis-ci.org/Xudong-Huang/may_actor.svg?branch=master)](https://travis-ci.org/Xudong-Huang/may_actor)
[![Build status](https://ci.appveyor.com/api/projects/status/5c0tbe3yaijpxxi3/branch/master?svg=true)](https://ci.appveyor.com/project/Xudong-Huang/may-actor/branch/master)
[![Current Crates.io Version](https://img.shields.io/crates/v/may_actor.svg)](https://crates.io/crates/may_actor)
[![Document](https://img.shields.io/badge/doc-may_actor-green.svg)](https://docs.rs/may_actor)


# may_actor

rust native actor library based on [may][may]

with this library
* you don’t need to declare messages that passed into the actor
* you don’t have to implement “actor” interface or trait for your actor.
* the actors will automatically have M:N scheduling powered by [may][may]

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

- send message via closure (`Actor.call`)

You can send messages to the actor with the `call` API. It accepts a closure that has the `&mut T` as parameter, so that you can change it’s internal state. The closure would be send to a queue inside the actor, and the actor would execute the closure by a coroutine that associate with it. This API would not block user’s execution and would return immediately.

- view actor internal state via closure (`Actor.view`)

You can also view the actor's internal state by the `view` API. It accepts a closure that has the `&T` as parameter, so that you can access the state without modify permission. The closure would be executed by the associated coroutine if there are no other pending messages. And it will block until the closure returns. So during the view stage, you are guaranteed that no others are modifying the actor.

- convert from raw instance reference to Actor (`Actor.from`)

You can transmute a &self type unsafely to it's handle type `Actor<T>`. This is convenient when need to get an actor handle in the implementation that need to pass as a function parameter.

However transmute from non actor context would trigger undefined behavior.

- Allow panic inside a closure message, and this would not kill the actor, the actor will continue to process successive messages.

- The actor can be cloned to get a new handle, this is just like how `Arc<T>` works, if all the actor handle got dropped, the associated coroutine will automatically exit.

## Notice
* Actor will catch panic and ignore the panic if any happened when processing a message, so there is no supervisor and restart policy right now. The actor only exit if all handles are dropped by user.
* Don't call thread block APIs in passed in closures, call [May][may] version APIs instead.
* This simple library doesn't support spawn actors across processes

## License

`may_actor` is licensed under either of the following, at your option:

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

<!--refs-->
[may]:https://github.com/Xudong-Huang/may