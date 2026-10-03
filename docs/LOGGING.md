# Logging
This article is about logging to the terminal in Rust language when the 
application is written with the Tokio asynchronous runtime.

> **TL;DR**  
> Logging to the terminal is a blocking I/O operation. Blocking is poison for a
> Tokio program, because Tokio runs thousands of tasks on just a handful of 
> worker threads — if one task blocks, it freezes every other task sharing that 
> thread. Our simple `logger.rs` solution solves this by having our tasks 
> **not** do the I/O at all. They just drop a message into a channel (a fast, 
> non-blocking operation), and a dedicated, separate OS thread does the actual 
> writing to the terminal. The async tasks never block, and the blocking 
> work happens on a thread that has nothing else to do.

But why blocking is dangerous in Tokio? Before understanding our simple 
`logger.rs` solution, we need to develop a good mental model about coding in 
Rust with the Tokio asynchronous runtime.

## Threads vs. Tasks
* An **OS thread** is a real, heavyweight execution unit managed by the 
  underlying OS - Operating System. 
* A **Tokio task** is a lightweight unit of asynchronous work (an 
  implementation of the `Future` trait which gets polled, and driven to 
  completion, by the Tokio runtime)
 
Tokio runs many tasks on few threads — typically one worker thread per CPU core.
So if we have 10,000 concurrent network requests, they're not running on 10,000
threads; they're multiplexed onto maybe 8 threads.

### Preemption vs. Cooperation
Another important difference between threads and tasks is how they're scheduled
to acquire the CPU.

* When the OS kernel is in charge of scheduling multiple threads competing 
  for the CPU, arbitrary **preempts** (forces) a thread to stop, whether the thread
  wants to or not.

* When the Tokio is charge, the task voluntarily **cooperates** by pausing itself 
  and yielding (handing) control back to the runtime.


#### Preemptive Scheduling (OS Threads)
In an operating system, the CPU uses hardware timer interrupts. Every few 
milliseconds, the OS kernel literally interrupts the CPU, freezes our running 
thread, saves its entire register and stack state, and swaps in another thread. 
The thread has zero say in when it gets paused. 

This strategy has pros and cons: a rogue loop (like `while true {}`) won't 
freeze the whole operating system because the OS will eventually interrupt it; 
but, preemption comes with high overhead. Switching an OS thread requires a 
kernel context switch, saving heavy register states, and flushing CPU caches.

#### Cooperative Scheduling (Tokio Tasks)
Tokio tasks run on top of standard OS threads. A single OS thread might manage 
thousands of Tokio tasks. Tokio cannot forcibly interrupt a task while it is 
actively executing CPU code; it must wait for the task to reach an explicit 
**yield** point. Yielding only happens at an `.await` point (or explicit calls
like `tokio::task::yield_now()`).

Also, this strategy has with pros and cons: extremely fast context switching, as
yielding is just an enum state transition with no kernel calls, no heavy stack 
saving; but if a task executes a CPU-heavy loop without an `.await`, it starves
the thread. Other tasks on that same worker thread will be blocked from running.

Therefore, Tokio achieves its efficiency only as long as tasks cooperate: a task
runs a little bit, then yields (e.g. at an `.await` point), letting another task
use the underlying thread. If a task performs a blocking operation — something
that makes the thread sleep, like a slow disk write, a mutex that isn't 
available, or `println!` waiting on a slow terminal — it blocks the entire
worker thread. Every other task scheduled on that thread now stalls. This is
often called _"starving the runtime"_ or _"blocking the reactor"_.

The `println!` might look innocent, but writing to `stdout` ultimately goes 
through the OS, and the OS can make the thread wait (slow terminal, piped 
output, full buffer, etc.). For logging in a hot path, that's a real risk.

### Task Channels
In Rust asynchronous programming, channels are synchronized queues used to send 
messages between concurrent tasks or threads. 

This is a powerful idea! Instead of sharing state by taking locks on shared 
memory (like a `Mutex`), channels allow tasks to communicate by passing messages
— embodying Rust’s core concurrency philosophy: _"Do not communicate by sharing 
memory; instead, share memory by communicating."_

A channel consists of two connected halves:

* **Sender** (`tx`): Pushes messages into the channel.
* **Receiver** (`rx`): Awaits and pops messages off the channel.

In either Tokio or `std::sync::mpsc`, pushing a message into a channel sends it 
across tasks without needing manual mutex locking. In async code, if a channel
is empty, awaiting `.recv().await` pauses the receiving task cooperatively 
without blocking the underlying OS thread until a sender yields a message.

#### Messages
The messages exchanged through a channel can represent either **events**, 
conveying data as payloads, or **commands** demanding for actions to be taken.

For example, our simple `logger.rs` solution represents messages as follows:

```rust
use std::time::SystemTime;

enum Severity { 
    Info, Warn, Error,
}

enum LogMessage {
    /* events   */ Record(String, Severity, SystemTime), 
    /* commands */ Flush, Shutdown,
}
```

The `Record` variant is an **event** message whose payload conveys data such as the
text of the log record, its severity (e.g. info, warning, or error) and the 
exact  timestamp at which the event occurred.

The `Flush` and `Shutdown` variant are **command** messages as they expect the 
receiver to take specific actions (do something).

#### MSPC Channels
MPSC stands for Multiple-Producer, Single-Consumer, and it's a channel which 
gets usually created during the initialization of our application:

```rust
use std::sync::mpsc;

pub fn init() {
    let (tx, rx) = mpsc::channel::<LogMessage>();
    // ...
}
```

MPSC describes a channel architecture where:

* **Multiple-Producer** (`tx` can be cloned)  
  You can have many separate worker tasks, threads, or web request handlers
  holding their own clone of the sender (`tx`). They all feed messages into the
  exact same shared channel.


*  **Single-Consumer** (`rx` cannot be cloned):   
   Exactly one task owns the receiver (`rx`). That single task sequentially 
   processes all incoming messages sent from any of the producers.

```text
Producer Task A (Tx) ──┐
                       │
Producer Task B (Tx) ──┼───► [ Channel Queue ] ───► Consumer Task (Rx)
                       │
Producer Task C (Tx) ──┘
```

MPSC channels excel whenever we need to funnel diverse concurrent activity into
a centralized, sequential manager.

1. **Centralized Resource Management & State Ownership**  
   In Rust, sharing mutable state across dozens of concurrent tasks requires 
   wrapping data in `Arc<Mutex<T>>`, which can lead to contention or deadlocks.
   With an MPSC channel, a single dedicated task owns the resource (e.g., 
   **stdout**, a database connection pool, a WebSocket connection, or an internal 
   state tree). Other tasks simply send command messages (e.g., 
   `LogMessage::Record` or `Command::SaveUser`) to that single consumer,
   eliminating mutex contention.


2. **Fan-In Work Distribution**  
   When multiple worker tasks operate concurrently (e.g., fetching data from 50 
   websites or processing parallel LLM prompt calls in Rig), each worker task 
   sends its individual completion result down cloned `tx` handles into a 
   single `rx` loop that collects, logs, or aggregates the results.


3. **Rate Limiting & Backpressure (Bounded MPSC)**  
   Tokio provides bounded MPSC channels, particularly the 
   `tokio::sync::mpsc::channel(buffer_size)`. If the sender produces messages
   faster than the single consumer can process them, `.send().await` on the 
   producer side will automatically pause once the buffer is full. This prevents
   fast producers from blowing up memory usage.


## Solution
Our `logger.rs` solution is designed to be **simple to use**: just initialize it, 
and then invoke the `info!` macro to produce log events with the given text, 
the appropriate  severity and the current timestamp: 

```rust
#[tokio::main]
async fn main() {
    let _g = logger::init();
    info!("Hello logger");
}
```

It is a basic implementation of a few ideas:

1. Adopt a channel to decouple producing log events from writing their messages.
2. Move the blocking work onto a thread that has nothing else to do.
3. Provide `info!()`, `warn!()` and `error!()` macros for convenience.

Let's look at how each piece of our code realizes these few ideas.


### Log Messages producers
First of all, thanks to the adoption of a MPSC - Multiple Producer Single 
Consumer channel, the production of log messages **never blocks**.

Crucially, this channel is the Rust standard library's unbounded channel, where 
_"unbounded"_ means that `tx.send()` never blocks — it just pushes the message 
onto an internal queue and returns immediately. Pushing may allocate more 
memory, but it doesn't wait until enough space is freed.

This is the heart of our performance claim. When our code calls the `info!`
macro, it ends up doing:

```rust
let _ = tx.send(LogMessage::Record(...));
```

where `send` is just a quick enqueue. It does no I/O, waits on nothing, and 
returns almost instantly. From Tokio's perspective, this is just normal, fast,
non-blocking work — it never puts a worker thread to sleep. 


### Log Messages Receiver
This is an ordinary OS thread (started by standard `thread::spawn`), not a
Tokio task. That's the point: it can afford to block, because it's not sharing
its thread with thousands of other tasks.

```rust
let handle = std::thread::spawn(|| receiver_loop(rx));
```

It's a single-purpose receiver whose only job is to keep taking messages 
from the channel queue and write log records out. The loop keeps calling 
`rx.recv()` at every iteration, and that's when the thread may eventually 
**block**. If the channel queue is empty, this thread simply sleeps until a 
message arrives. That's perfectly fine here, because this thread has nothing 
else to do anyway.

```rust
fn receiver_loop(rx: Receiver<LogMessage>) {
    
    while let Ok(msg) = rx.recv() {
        // process the message ...
    }
}
```

#### Locking StdOut
This is a performance optimization applied in the receiving loop when it tries to
write the log record out, very often to the underlying `stdout`, for example 
via the `writeln!`macro.

Normally, every `writeln!` invocation locks `stdout`, writes, then unlocks it.
If many threads logged directly, they'd contend on that lock, and each write 
would pay locking overhead. By locking once at the start of the receiving loop 
and holding it for the thread's whole lifetime:

```rust
fn receiver_loop(rx: Receiver<LogMessage>) {
   
   let mut stdio = io::stdout().lock();

   while let Ok(msg) = rx.recv() {
      match msg {
         LogMessage::Record(text, severity, ts) => {
            let _ = writeln!(stdio, "{}", text);
         }
         // ... handle other messages here ...
      }
   }
}
```

avoids lock contention entirely (only one thread ever writes), and avoids 
repeated lock/unlock overhead.


### Actual Send and Macros
Following is the `send_record` function that actually sends a log event message
to the channel.


```rust
use std::time::SystemTime;

pub fn send_record(severity: Severity, text: String) {
    if let Some(tx) = GLOBAL_SENDER.get() {
        let _ = tx.send(LogMessage::Record(text, severity, SystemTime::now()));
    }
    else {
        // Fallback to standard println if logger isn't initialized yet
        println!("{}", text);
    }
}
```

If the logger is initialized, it enqueues the message. If not (e.g. someone logs
before the logger was initialized), it falls back to plain `println!` so the
message isn't lost. Note that the `format!` work (building the string) happens
on the caller's thread, not the logger thread.

#### Log Macros
The `info!`, `warn!` and `error!` macros are convenience wrappers: they format
the message on the caller's thread and hand it to `send_record` with the
matching severity, so the caller never touches the channel directly.

Under normal builds (`cfg(not(test))`), each macro expands to:

```rust
#[cfg(not(test))]
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::logger::send_record($crate::logger::Severity::Info, format!($($arg)*))
    };
}
```

`warn!` and `error!` are identical, passing `Severity::Warn` and
`Severity::Error` respectively. The macros are exported with `#[macro_export]`
and brought into scope in `main.rs` via `#[macro_use] mod logger;`, so they
"just work" anywhere in the crate without imports.


#### Conditional Compilation
Is a feature that allows us to include or exclude code from being compiled based
on specific conditions — such as the target operating system, target 
architecture, active Cargo feature flags, or compiler settings (like `test` or 
`debug_assertions`).

Unlike C/C++ which uses a text-based preprocessor (`#ifdef`), Rust uses the
compiler built-in `#[cfg(...)]` attributes and the `cfg!` macro, which are 
fully integrated into the language grammar and syntax checking.

```rust
#[cfg(test)]
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => { 
        // Trick it to avoid unused warnings
        if false {
            let _ = format!($($arg)*);
        }
    };
}
```

Under `cfg(test)`, the log macros expand to nothing. This keeps test output
clean and removes logging overhead from tests entirely. The `cfg(not(test))`
versions are the "real" ones used in production builds.



#### The Global Sender
Without it, we'd have to propagate a sender (`tx`) clone through every struct, 
function, and task that wants to log events. Making it global lets the 
`info!`/`warn!`/`error!` macros "just work" anywhere.

The solution is to place a sender clone into a `static OnceLock`, as that's a 
widely used and valid pattern in Rust applications for cross-cutting concerns 
like logging, metrics, or telemetry.

```rust
use std::sync::OnceLock;
static GLOBAL_SENDER: OnceLock<Sender<LogMessage>> = OnceLock::new();
```

The Rust `OnceLock` synchronization provides a thread-safe, write-once cell. It 
allows initializing a global static variable lazily at runtime (e.g., during 
application startup). Its `.set()` method runs once and stores the owned clone
of the `Sender<LogMessage>`. Any thread or async task in our application can call 
`GLOBAL_SENDER.get()` to retrieve a reference to sender, and just use it.

Moreover, accessing a `OnceLock` via `.get()` does not require acquiring a 
mutex. It is a lock-free atomic read (not a mutex lock). So the hot path — 
checking for the sender on every log call — is extremely cheap.


### Init
Our logger solution needs to be initialized only once. That's when the task 
channel is created, the logger thread is spawned (with its loop), the global 
sender and the guard are wired up.

```rust
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

pub fn init() -> LoggerGuard {
   
   // Create the channel
   let (tx, rx) = mpsc::channel::<LogMessage>();

   // Spawn the logger thread with the logger loop
   let handle = thread::spawn(receiver_loop(rx));

   // Wire up the global sender and the guard
   GLOBAL_SENDER.set(tx.clone());
   LoggerGuard { tx, handle: Some(handle) }
}
```

### Clean Shutdown
Let's recall how the `main` function initializes our simple logger solution 
and let's explain why it holds a `LoggerGuard` value in scope:

```rust
fn main() {
    let _guard = logger::init();
    info!("Hello logger");
  
    // do what the program is supposed to do ...
    
    // ... then automatically drop the logger guard
}
```

#### The Guard Idiom
In Rust, a so-called **guard** is an idiom that leverages Rust's ownership
system and **RAII** (Resource Acquisition Is Initialization) to automatically
manage resources, enforce safety rules, or clean up state when a value goes out 
of scope. Instead of relying on manual cleanup calls (like `lock.unlock()` or 
`file.close()`), Rust encapsulates access to a resource inside a temporary 
"guard" object. When that guard object is destroyed at the end of its scope, 
its `Drop` implementation runs automatically to perform cleanup.


```rust
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

pub struct LoggerGuard {
   tx: Sender<LogMessage>,
   handle: Option<JoinHandle<()>>,
}

impl Drop for LoggerGuard {
   fn drop(&mut self) {
      let _ = self.tx.send(LogMessage::Flush);
      let _ = self.tx.send(LogMessage::Shutdown);
      if let Some(handle) = self.handle.take() {
         let _ = handle.join();
      }
   }
}
```

The channel is unbounded, so messages are buffered in memory. If the program
just ended, those buffered messages might never get written. The guard fixes
that:

* it sends the `Flush` command  
  the receiver thread will call `stdio.flush()` to push out  anything 
  sitting in the underlying OS buffer;

* it sends the `Shutdown` command  
  the receiver thread breaks out of its receiving loop and finally exits;


* it calls `handle.join()`  
  the main thread waits for the receiver thread to finish before the program 
  terminates.

Because `LoggerGuard` implements `Drop`, all of this happens automatically when
the guard goes out of scope (typically when the `main` returns). This 
guarantees we never lose log output on shutdown and all threads are gracefully
terminated.


#### The "Disarming" Pattern
In Rust, if a struct implements the `Drop` trait, the compiler forbids you from
moving non-copy fields out of that struct. For example, focus on the following
naive implementation:

```rust
struct MyGuard {
    resource: String, // Cannot be moved out directly!
}

impl Drop for MyGuard {
    fn drop(&mut self) {
        println!("Guard dropped!");
    }
}

impl MyGuard {
    fn consume(self) {
        // error[E0509] : cannot move out of type `MyGuard`, which implements 
        // the `Drop` trait
        let r = self.resource;
        // ...
    }
}
```

Why the compiler blocks this? The answer is that when `self` is consumed and 
goes out of scope, Rust expects to run its `drop(&mut self)` method. If we were 
allowed to move `resource` ownership out of the struct value, the drop method 
would end up operating on uninitialized or invalid memory!

The solution is wrapping the `resource` field inside an `Option`, so we can 
provide a valid "empty" state (`None`) that can remain inside the struct while 
we extract the actual inner value (as `Some(T)`).

A much better implementation of the guarding idiom is as follows:

```rust
use std::thread::JoinHandle;

pub struct MyGuard {
    // The guarded resource is wrapped inside an Option envelope 
    // so we can safely extract or disarm it
    handle: Option<JoinHandle<()>>,
}

impl Drop for MyGuard {
    fn drop(&mut self) {
        // When dropping, if the resource is still inside, take it!
        if let Some(handle) = self.handle.take() {
            // finally use the resource
            let _ = handle.join();
        }
    }
}
```

The `.take()` method does two things atomically:

1. Replaces the value inside the `Option` with `None`.
2. Returns the original `Option<T>` (containing our resource value, such as 
   the thread handle), effectively giving us full ownership.

This is also known as the _"disarming guards"_ pattern because, bery often a 
guard is  designed to execute cleanup unless an operation completes 
successfully. We use `.take()` to extract the resource and set it to `None`, 
effectively _"disarming"_ the guard so its `Drop` implementation becomes a 
harmless no-op.


### Caveats and Tradeoffs
A good engineer should also know the limits of this approach:

* **Unbounded channel = unbounded memory**  
  If log producers outpace the writer, messages pile up in memory without limit.
  This is fine for modest logging, but a real production logger might use a 
  bounded channel or drop messages under pressure.

* **Per-message allocation**  
  Each log event allocates a string, severity and timestamp. Then, in addition
  to that the channel may also allocate for the queue node. Again, fine for 
  most cases, but not zero-cost.

* **Single consumer = potential bottleneck**  
  If we log a lot, one thread writing to a slow terminal can become the 
  throughput limit. (But that's exactly the tradeoff that keeps ordering simple
  and correct.)

* **Global mutable state**   
  The global OnceLock is convenient but means only one logger can exist per 
  process, and it's harder to test in isolation (hence the cfg(test) macros 
  compiling logging away).


## Conclusions
Asynchronous code should never block. When we must do blocking work, hand it 
to a  dedicated thread and communicate through a channel.

Our simple `logger.rs` solution is a clean, concrete example of that principle. 
The next time we see blocking work inside an async function (file I/O, a slow 
library call, `std::thread::sleep`, etc.), the same pattern applies: move it to 
a worker thread and talk to it through a channel — or, in Tokio specifically, 
use `tokio::task::spawn_blocking` for the same idea.
