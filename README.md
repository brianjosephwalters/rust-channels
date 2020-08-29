# Channel implementation in Rust
Following along with [Crust of Rust: Channels]()

## Channels
Std library has a built in mechanism for channels.  Also Crossbeam.  Parking Lot provides Mutex and Condvar as well.

Have receiver and sender types.  When you create a channel, you create a sending handle and a receiving handle.
* They can be moved independently.

Multi-Producer, Single Consumer (MPSC)
* Channels are uni-directional.
* Clone sender, but not receiver

Take a generic parameter T.  Can send any T for a channel.

Similar to Go channels and function in the similar ways.  Other languages have many-to-many channels.


## First Commit - Scaffolded Channel
Using other parts of the sync module.
* `Mutex` is a lock. - Lock method returns a Guard.  While you have that guard, you are guaranteed to be the only thing that can access that T protected by the mutex.
* `Arc` - a reference counted type.  Atomically Refernece Counted type.  Needed to work across thread boundaries.
* `Condvar` - Conditional Varaible - a way to announce to a different thread that you've changed something it cares about.  Wakes up thread that was sleeping telling it there is somethign to read.

We use an `Inner` class to hold the data - usually some kind of queue.  

Sender and Receiver will hold a reference to the same `Inner`.

RefCell does runtime borrow-check like Mutex, but Mutex will block one thread if two try to access at the same time.  RefCell will simply respond with an error and let the program move on.

Mutex goes inside the Inner. 

__Why does the receiver need a mutex if there is only one receiver?__
The Send and Receive may happen at the same time.  They need to be mutually exclusive to each other as well.  So they all need to synchronoize with the mutex.

__Why not just use a boolean semaphore?__
A mutex is a boolean semaphore.  Mutex buys you integration with the parking mechanism and User Mode stuff implemented by the OS.  A boolean semaphore is just a flag you check and atomically update.  If the flag is set (someone has the flag) you have to spin and continually check it.  With a mutex, the OS can put the thread to sleep and wake it up when the mutex is available.

__Why is the Arc needed?__
The sender and receiver would have two different instances of Inner.  If they did, how would they communicate?  They need to share that Inner.

## Second Commit - Mutex and Condvar
When taking the lock, the previous holder could have paniced while holding the lock.  LockResult will either hold the Guard or a PoisonError<Guard>.  The latter lets us know that the other thread paniced.  You can decide whether you care that the previous thread paniced.

**First problem:** It's not a queue. Right now we only have a Vec<T>.  In theory, you can remove first, but then you have hte overhead of shifting elements left.  In practice, you often use what is called a ring buffer.  But for now we'll use the `VecDeque`.
* Don't want to 'swap-remove', that will cause the last thing sent to become the first thing received.  So that's more like the stack behavior.

`VecDeque` is a fixed amount of memory, but tracks the start and end separately.  Poping from front, removes the elements and moves the pointer to where the data starts.  Data ends up wrapping around the whole thing.  Can be used as a queue instead of a stack.

**Second problem:** Receiving returns an Option.  Could be that there is nothing in the Deque.  We could return a Try-Receive method, but really we want to provide the T.  So we need a blocking version of `recv` - If there isn't something there yet, the receiver waits for something to be in the channel.  This is where the `Condvar` comes into play.

Convar needs to be separate from the Mutex.  Imagine you were currently holding the mutex and you needed to wake other people up.  The person you wake up has to take the Mutex.  If you tell them to wake up while you hold the mutex, then they wake up and try to take the mutex.  They can't, so they go back to sleep.  Deadlock.

The Condvar also needs to be given the mutex before you wait - `self.inner.available.wait(queue).unwrap()`.  Condvar ensures that the waiting thread doesn't go to sleep while holding the Mutex.  It also takes the Mutex for you when it wakes up. (That's also why we re-assign to the guard `queue` and let it return the T on the next interation of the loop.)

For this problem, we'll need to use a `loop`.  
*  Since the Mutex is automatically given to you when you wake up, then you don't need to retrieve the Mutex inside the loop.  
*  This isn't a spin loop -  If we end up in the None clause, we wait for a signal on the available Condvar.  The OS will put the thread to sleep and only wake it up when there is a reason to - the Inner has become available.

Now the Sender needs to notify the receiver once it sends.  We need to drop the lock and then notify so that the Receiver can get the lock.  
*  Since there is only one Receiver, we know that we will be waking up that Receiver.
*  The lock would automatically be dropped after the notify, but we want the receiver to be able to immediately take the lock once it is awoken.

With Condvar, the OS doesn't guarantee you are woken up with something to do.  That's what the loop does.

You can use brackets to scope the holding of the mutex, like so:
```rust
let queue = self.inner.queue.lock().unwrap();
queue.push_back(t);
drop(queue);
self.inner.available.notify_one();
```
is equivalent to:
```rust
{
    let queue = self.inner.queue.lock().unwrap();
    queue.push_back(t);
}
self.inner.available.notify_one();
```

Similarly, when we `return t` in the `recv()` method, this causes the guard `queue` to go out of scope and so the mutex on the channel is implicitly released.

In the current design, there are never any waiting Senders.

## Commit 3 - Mutability and Cloning
The `queue` needs to be updated to be mutable since we are adding and removing from the `VecDeque`.

Our Sender also needs to be Cloneable.  But `derive(Clone`) desugars into:
```rust
impl<T: Clone> Clone for Sender<T> {
    fn clone(&self) -> Self {...}
}
```
This implementation automatically ensure T is bound to Clone as well.  So when we clone a Sender, we also clone the T.  But our this case, the clones of Sender should still be working on the same inner `VecDeque`.
Arc implements Clone regardless of the inner type - That's what reference counting means. We can clone an Arc, but there is still only one of the things inside.

So we need to implement our own Clone.
```rust
Sender {
    inner: self.inner.clone(),
}
```
Often, you want something like this. But in our case, `Arc` auto-dereferences (via the dot operator) to the inner type `T`.  So Rust won't be able to tell whether we want to Clone the `Arc` or the inner type `T`.  So we instead we call Clone using the class method so that the compiler knows we are want to use Arc's clone, not T's clone.
```rust
Sender {
    inner: Arc::clone(&self.inner),
}
```


## Commit 4 - Preventing blocking when all Senders drop
Imagine there are no senders left.  What should the receiver do if there are no senders left?  There can never be any future sender. (In order to get a Sender, you have to clone a Sender.  Once they are gone, there is no way to get another one.)

We need a way to indicate to the Receiver that there are no more Senders left.  The channel has been closed.  We want some additional data guarded by the Mutex.

Every time you clone a Sender, you increase the number of senders.  Every time you drop a Sender, you need to decrement the count.  In the latter case, though.  If we were the last sender to run, then we need to wake up the other senders.

The Receiver now needs to return an Option<T> instead of a T.  If the channel is empty forever, it needs to be able to return None.

__Couldn't we use the refrence count in the Arc instead?__
Gets a little complicated because of weak references.  Since we are only using strong references, maybe:
```rust
    None if Arc::strong_count(&self.shared) == 1 => return None,
```

Tells you how many instances of that Arc there are.  If only 1, then it must be the one of the Receiver, which means there are no Senders.  **But** the complicated case - if you drop a Sender, you don't know whether to notify.  If the count is 2, you might be the last sender, or you may be the second to laster sender and the receiver has been dropped. 

__Why not use atomic usize?__
Since we have to take the Mutex anyways, why not just update the count under the mutex.  Atomic usize doesn't save us anything.

__Debugging__
Run your tests like this:
```bash
cargo t --test-threads=1 --nocapture
```
In a test, you can use `eprintln!()` to print on the error return of the test.  Helps to determine which line failed.
```rust
eprintln!("X");
eprintln!("drop sender, count was {}", inner.senders);
```


In your code, you can use a `dbg!()` macro to print the value:
```rust
    None if dbg!(inner.senders) == 0 => return None,
```

## Commit 5
The problem can occur the other way around.  What if we drop the Receiver and then try to do a Send.  It's not really a problem.  The test will run just fine, but should it?  Should we be told that the channel has closed rather than the send just blindly succeeding.
* If you do want it to handle failure, the send should give back the value that the user tried to send. So they can try to send it somewhere else or log it.
* Add a close flag to the inner.  If the receiver drops, the closed flag is set and there is a notify_all().  (Although senders don't block in this implementation).  And the send method just returns an error if the flag is set, rather than pushing to the queue.

__Design Decisions__
1.  Every operation takes the lock. Fine if the channel doesn't need to be high performance.  May not want the Sends to content with one another.  The only thing that really needs to be synchronized is the Senders with the Receiver.
2.  The standard library channel as a receiver and two different Sender types.  Sender and SyncSender.  one is synchronous and the other is asynchronous - Not the same as Async.  Whether the channel forces the Sender and Receivers to synchronize.
    Imagine a sender that is much faster than a Receiver.  In the current design, the queue would keep growing.  Sender and Receiver go in lockstep.  At some point the channel would fill up and the Sender would block.  If The sender is too fast, nothing in the system is told that the system isn't keeping up.So the question is really whether `sends()` can block.

The advantage of a synchronous channel is that there is **back-pressure**.  The Sender will eventually start blocking if the channel fills up.
* Now the Receiver may have to notify the Sender and tell it to start sending more.

You basically need two Convars. (guarded by same mutex?)
* One to notify the Receiver the way we currently have it.
* One to notify the Sender when the receiver is caught up.

Now are Channel method would need to take a capacity.
* Std library has a sync_channel that takes a capacity.  This one returns a SyncSender.  So you can tell this one uses backpressure.


__Why not have the Senders use Weak to handle backpressure scenarios?__
Weak is a version of Arc that doesn't increment the reference count.  But you have a way to try to increment the reference count if it hasn't already gone to zero.  So the sender would try to upgrade their sender.  If they succeed, then they know the Receiver is still there and can try to send.
* Every time you try to send, you have to atomically update the reference count and decrement it after.  So there is overhead.

__Is there a way to have a Condvar without a Mutex?__
No - the Condvar wait() requires you to have a Mutex guard.

__Wouldn't send() technically block if the Vec does a resize?__
The call to push_back() is not free.  But it's not technically blocking.  The send() just takes longer.  In the meantime, you can't do sends and receives.  In practice, you don't use a VecDeque anyways.

__How hard is it to implement an Iterator for Receiver?__
```rust
impl<T> Iterator for Receiver<T> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        self.recv()
    }
}
```

## Commit 6
Because we know there is only one Receiver, we don't really need to take the lock for every recv().
