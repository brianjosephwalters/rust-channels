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
