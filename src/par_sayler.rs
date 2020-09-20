use std::collections::HashMap;
use md5;
use rayon::prelude::*;
use std::sync::mpsc;
use std::thread;
use std::time;

pub struct SaylerResult {
    pub inputs: (u128, u128),
}

/// Parallel Sayler n-Collision finding for MD5
///
/// A Sayler n-Collision is a pair of distinct inputs whose md5sum matches in the first n and last
/// n printed hex characters. For example this is a Sayler-6 Collision:
///
/// $ md5sum file1
/// d41d8ce1987fbb152380234511f8427e  file1
/// $ md5sum file2
/// d41d8cd98f00b204e9800998ecf8427e  file2
pub fn find_collision(n: u8) -> Result<SaylerResult, &'static str> {
    if n > 16 {
        return Err("There are only 32 hex digits in an MD5 hash, therefore there are no collisions for n > 16");
    }

    // TODO: handle non-even n which would require tracking arrays of 4 bits rather than the arrays
    //  of bytes that the MD5 digest produces
    if n % 2 != 0 {
        return Err("Only even n values are currently supported")
    }

    return parallel_find_collision(n);
}

fn parallel_find_collision(n: u8) -> Result<SaylerResult, &'static str> {
    // rayon::ThreadPoolBuilder::new().num_threads(1).build_global().unwrap();

    let n_bytes: usize = (n / 2) as usize;
    let tail_index: usize = (128 / 8) - n_bytes;

    let (worker_tx, worker_rx) = mpsc::sync_channel(25);
    let (checker_tx, checker_rx) = mpsc::channel();

    // Rayon's par_iter blocks so we need to spawn the tracker in another thread before running the workers
    thread::spawn(move || hash_tracker(worker_rx, checker_tx, n as usize));

    (0..std::u128::MAX).into_par_iter()
        .for_each_with(WorkerInit {tx: worker_tx, n: n as usize, n_bytes, tail_index}, hash_worker);

    let result = checker_rx.recv();

    return match result {
        Ok(result) => result,
        // TODO: should propagate errors
        Err(_) => Err("Error while fetching result"),
    }
}

struct WorkerResult {
    result: Vec<u8>,
    input: u128,
}

#[derive(Clone)]
struct WorkerInit {
    tx: mpsc::SyncSender<WorkerResult>,
    n: usize,
    n_bytes: usize,
    tail_index: usize,
}

fn hash_worker(init: &mut WorkerInit, input: u128) {
    let digest = md5::compute(input.to_be_bytes());

    // Note: ideally we should be able to use static arrays, however support for generic
    //  constants is needed to do this (implementation in progress at
    //  https://github.com/rust-lang/rust/issues/44580)
    let mut entry: Vec<u8> = Vec::with_capacity(init.n);
    // Since we're looking for Sayler collisions, we only care about the first and last n/2 bytes
    entry.extend_from_slice(&digest[0..init.n_bytes]);
    entry.extend_from_slice(&digest[init.tail_index..]);

    match init.tx.send(WorkerResult { input, result: entry }) {
        Err(e) => eprintln!("Worker error while sending result: {}", e),
        Ok(_) => (),
    }
}

fn hash_tracker(worker_rx: mpsc::Receiver<WorkerResult>, result_tx: mpsc::Sender<Result<SaylerResult, &'static str>>, n: usize) {
    const TIMED_ITERATIONS: u32 = 1000000;

    // We expect a collision after roughly 2^(4*n) inputs due to the birthday paradox
    let expected_collision_at: usize = 2 ^ (4 * n);

    // We pre-allocate a hashmap with ~.5x extra headroom to avoid needing to resize the heap
    let mut hashes: HashMap<Vec<u8>, u128> = HashMap::with_capacity(expected_collision_at * 3 / 2);
    let mut collision: Option<SaylerResult> = None;

    let start = time::Instant::now();
    let mut loop_start = time::Instant::now();

    for item in worker_rx.iter() {
        match check_hash(item, &mut hashes) {
            Some(result) => {
                collision = Some(result);
                break;
            }
            None => (),
        }

        if hashes.len() % (TIMED_ITERATIONS as usize) == 0 {
            let iter_rate = (TIMED_ITERATIONS as f32) / loop_start.elapsed().as_secs_f32();
            eprintln!("Reached {} iterations, running {} iterations / s", hashes.len(), iter_rate);
            loop_start = time::Instant::now();
        }
    };

    eprintln!("Completed running {} hashes in {} seconds", hashes.len(), start.elapsed().as_secs_f32());

    result_tx.send(
        match collision {
            None => Err("No collisions found"),
            Some(result) => Ok(result),
        }
    ).unwrap();
}

fn check_hash(item: WorkerResult, hashes: &mut HashMap<Vec<u8>, u128>) -> Option<SaylerResult> {
    if hashes.contains_key(item.result.as_slice()) {
        let previous_input = *hashes.get(item.result.as_slice()).unwrap();
        return Some(SaylerResult {inputs: (previous_input, item.input)});
    }

    hashes.insert(item.result, item.input);
    return None;
}