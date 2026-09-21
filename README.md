
# Concurrency and Liveness

The synchronization helpers have these usage considerations:

- `broadcast::Sender::send` snapshots subscriptions before invoking delivery callbacks, so broadcast callbacks are not called while the subscriptions lock is held. Callbacks may still recurse indefinitely or deadlock through unrelated user-owned locks.
- `callback_event::Sender::trigger` invokes callbacks while holding its callback collection lock. Callbacks must not call `trigger`, `subscribe`, or drop a subscription for the same sender.
- `AsyncItem::read` returns a read guard. Drop the guard before awaiting `set`, `unset`, or another operation that may need the write lock:

	```rust
	let value = item.read().await;
	let copied = *value;
	drop(value);
	item.set(copied).await;
	```

- `AsyncItem::read` uses notification-based waiting and should not be treated as a general condition-variable primitive. A notification can occur between the availability check and waiter registration.
- `mpcc` may drop queued values while holding its queue lock. A queued value's `Drop` implementation should not re-enter that queue or block on a lock whose owner needs the queue.
