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


## First Commit
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

