//! ObjectPool micro benchmarks: `cargo bench --bench object_pool`
//!
//! Every run performs `N` operations on a pool of `N` objects; the median of `RUNS` runs is printed.

use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use bytifex_utils::containers::object_pool::{DefaultObjectPoolIndex, ObjectPool};

type Pool = ObjectPool<u64, DefaultObjectPoolIndex>;

const N: usize = 100_000;
const RUNS: usize = 31;

fn bench(name: &str, mut run: impl FnMut() -> Duration) {
    let mut times: Vec<Duration> = (0..RUNS).map(|_| run()).collect();
    times.sort();
    let median = times[RUNS / 2];
    println!(
        "{name:<32} {:>7.2} ns/op",
        median.as_nanos() as f64 / N as f64
    );
}

fn main() {
    let mut rng = fastrand::Rng::with_seed(1);

    bench("create_object", || {
        let mut pool = Pool::new();
        let start = Instant::now();
        for value in 0..N as u64 {
            black_box(pool.create_object(value));
        }
        start.elapsed()
    });

    bench("create_object_with_fn", || {
        let mut pool = Pool::new();
        let start = Instant::now();
        for value in 0..N as u64 {
            black_box(pool.create_object_with_fn(|_| Ok::<_, ()>(value)).is_ok());
        }
        start.elapsed()
    });

    let mut pool = Pool::new();
    let mut indices: Vec<_> = (0..N as u64)
        .map(|value| pool.create_object(value))
        .collect();
    rng.shuffle(&mut indices);

    bench("get_ref (random order)", || {
        let start = Instant::now();
        for &index in &indices {
            black_box(pool.get_ref(index));
        }
        start.elapsed()
    });

    bench("get_mut (random order)", || {
        let start = Instant::now();
        for &index in &indices {
            if let Some(value) = pool.get_mut(index) {
                *value += 1;
            }
        }
        start.elapsed()
    });

    bench("iter", || {
        let start = Instant::now();
        black_box(pool.iter().sum::<u64>());
        start.elapsed()
    });

    // releases every object in random order, then fills the pool again from the free slots
    bench("release + create (per pair)", || {
        let start = Instant::now();
        for &index in &indices {
            black_box(pool.release_object(index));
        }
        for (index, value) in indices.iter_mut().zip(0..) {
            *index = pool.create_object(value);
        }
        let elapsed = start.elapsed();

        rng.shuffle(&mut indices);
        elapsed
    });
}
