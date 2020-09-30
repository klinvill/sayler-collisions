use std::collections::HashMap;
use md5;
use rayon::prelude::*;
use std::sync::mpsc;
use std::thread;
use std::time;
use std::convert::TryInto;
use std::cmp::min;
use md5::Digest;
use std::borrow::Borrow;

use super::sayler::SaylerResult;

// TODO: Memory efficient implementation found here on page 3: https://eprint.iacr.org/2012/731.pdf
// TODO: Could parallelize using OpenCL and the kernel found here: https://github.com/sghctoma/oclcrack/blob/master/MD5.cl

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

/// Returns the 0-indexed partition number a value belongs in
///
/// # Arguments
/// [`stride`]: stride between partitions
/// [`partitions`]: number of partitions
fn partition(value: u128, stride: u128, partitions: usize) -> usize {
    return min((value / stride) as usize, partitions-1);
}

/// Computes the Sayler value from an md5 hash
///
/// A Sayler value is defined here as a 128-bit unsigned integer derived from the first n and last n
/// hex values from an md5 hash. Sayler values have a 1-to-1 mapping with u128 values.
fn extract_sayler_value(hash: Digest, n: usize) -> u128 {
    // TODO: handle non-even n which would require tracking arrays of 4 bits rather than the arrays
    //  of bytes that the MD5 digest produces
    let n_bytes = n / 2;

    // Note: ideally we should be able to use static arrays, however support for generic
    //  constants is needed to do this (implementation in progress at
    //  https://github.com/rust-lang/rust/issues/44580)
    let mut entry: Vec<u8> = Vec::with_capacity(n);
    // Since we're looking for Sayler collisions, we only care about the first and last n/2 bytes
    entry.extend_from_slice(&hash[0..n_bytes]);
    entry.extend_from_slice(&hash[16 - n_bytes..]);

    // pad with zeros so we can create a u128 value
    for _ in entry.len()..16 {
        entry.push(0);
    }

    // Note: since we're looking for a hash collision, it doesn't really matter that the hash
    // value matches the derived u128 value (hence the use of little endianness for convenience)
    // as long as there's a 1-to-1 mapping between hash values and u128 values.
    let int_val = u128::from_le_bytes(entry.as_slice().try_into().unwrap());
    return int_val;
}

#[derive(Clone)]
struct OptimizedImplementation {
    n: u8,
    spawn_workers: fn(threads: usize, reader_txs: Vec<mpsc::SyncSender<WorkerResult>>, stride: u128, partitions: usize),
}

#[derive(Clone)]
struct ReaderInit {
    result_tx: mpsc::Sender<Result<SaylerResult, &'static str>>,
    n: usize,
}

struct WorkerResult {
    result: u128,
    input: u128,
}

#[derive(Clone)]
struct WorkerInit {
    reader_txs: Vec<mpsc::SyncSender<WorkerResult>>,
    stride: u128,
    partitions: usize,
    n: usize,
}

fn parallel_find_collision(n: u8) -> Result<SaylerResult, &'static str> {
    let optimized_implementations: HashMap<u8, OptimizedImplementation> =
        [
            (6, OptimizedImplementation{ n: 6, spawn_workers: spawn_workers_6 }),
            (10, OptimizedImplementation{ n: 10, spawn_workers: spawn_workers_10 }),
        ].iter().cloned().collect();

    let optimized_impl = optimized_implementations.get(&n);

    let worker_to_reader_ratio: usize = if optimized_impl.is_some() {1} else {2};
    // TODO: pull dynamically or allow user to specify
    const CORES: usize = 16;
    const BUFFER_SIZE: usize = 128;

    let reader_threads = CORES / (worker_to_reader_ratio + 1);
    let worker_threads = reader_threads * worker_to_reader_ratio;

    let lower_sayler_bound: u128 = 0;
    let mut upper_sayler_bound_bytes:Vec<u8> = Vec::new();
    for _ in 0..n {
        upper_sayler_bound_bytes.push(u8::max_value());
    }
    for _ in n..16 {
        upper_sayler_bound_bytes.push(u8::min_value());
    }
    let upper_sayler_bound = u128::from_le_bytes(upper_sayler_bound_bytes.as_slice().try_into().unwrap());

    let stride = (upper_sayler_bound - lower_sayler_bound) / (reader_threads as u128);

    let result = match optimized_impl {
        Some(optimized) => spawn_threads_optimized(
            reader_threads,
            worker_threads,
            stride,
            BUFFER_SIZE,
            optimized.clone(),
            ),
        None => spawn_threads(
                reader_threads,
                worker_threads,
                stride,
                BUFFER_SIZE,
                n as usize,
            ),
    }.recv();

    return match result {
        Ok(result) => result,
        // TODO: should propagate errors
        Err(_) => Err("Error while fetching result"),
    }
}

fn spawn_threads(reader_threads: usize, worker_threads: usize, stride: u128, buffer_size: usize, n: usize) -> mpsc::Receiver<Result<SaylerResult, &'static str>> {
    let (result_tx, result_rx) = mpsc::channel();

    // Rayon's par_iter blocks so we need to spawn readers and workers in separate threads
    let (reader_txs, reader_rxs) = (0..reader_threads).map(|_| mpsc::sync_channel(buffer_size)).unzip();
    thread::spawn(move || spawn_readers(reader_threads.clone(), result_tx, reader_rxs, n.clone()));
    thread::spawn(move || spawn_workers(worker_threads, reader_txs, stride, reader_threads, n));

    return result_rx;
}

fn spawn_threads_optimized(reader_threads: usize, worker_threads: usize, stride: u128, buffer_size: usize, optimized_impl: OptimizedImplementation) -> mpsc::Receiver<Result<SaylerResult, &'static str>> {
    let n: usize = optimized_impl.n.clone() as usize;

    let (result_tx, result_rx) = mpsc::channel();

    // Rayon's par_iter blocks so we need to spawn readers and workers in separate threads
    let (reader_txs, reader_rxs) = (0..reader_threads).map(|_| mpsc::sync_channel(buffer_size)).unzip();
    thread::spawn(move || spawn_readers(reader_threads.clone(), result_tx, reader_rxs, n));
    thread::spawn(move || (optimized_impl.spawn_workers)(worker_threads, reader_txs, stride, reader_threads));

    return result_rx;
}

fn spawn_readers(
    threads: usize,
    result_tx: mpsc::Sender<Result<SaylerResult, &'static str>>,
    reader_rxs: Vec<mpsc::Receiver<WorkerResult>>,
    n: usize
) {
    let reader_pool = rayon::ThreadPoolBuilder::new().num_threads(threads);
    reader_pool.build().unwrap().install(|| {
        (reader_rxs).into_par_iter()
            .for_each_with(ReaderInit {result_tx, n}, reader_thread);
    })
}

fn reader_thread(init: &mut ReaderInit, rx: mpsc::Receiver<WorkerResult>) {
    let cloned_tx = mpsc::Sender::clone(&init.result_tx);
    hash_tracker(rx, cloned_tx, init.n);
}

fn spawn_workers(threads: usize, reader_txs: Vec<mpsc::SyncSender<WorkerResult>>, stride: u128, partitions: usize, n: usize) {
    let worker_pool = rayon::ThreadPoolBuilder::new().num_threads(threads);
    worker_pool.build().unwrap().install(|| {
        (0..std::u128::MAX).into_par_iter()
            .for_each_with(WorkerInit {reader_txs, stride, partitions, n}, hash_worker);
    });
}

fn hash_worker(init: &mut WorkerInit, input: u128) {
    let digest = md5::compute(input.to_be_bytes());
    let sayler_value = extract_sayler_value(digest, init.n);

    let reader = partition(sayler_value, init.stride, init.partitions);
    let tx = init.reader_txs[reader].borrow();
    match tx.send(WorkerResult { input, result: sayler_value }) {
        Err(e) => eprintln!("Worker error while sending result to reader {}: {}", reader, e),
        Ok(_) => (),
    }
}

fn hash_tracker(worker_rx: mpsc::Receiver<WorkerResult>, result_tx: mpsc::Sender<Result<SaylerResult, &'static str>>, n: usize) {
    const TIMED_ITERATIONS: u32 = 1000000;

    // We expect a collision after roughly 2^(4*n) inputs due to the birthday paradox
    let expected_collision_at: usize = 2 ^ (4 * n);

    // We pre-allocate a hashmap with ~.5x extra headroom to avoid needing to resize the heap
    let mut hashes: HashMap<u128, u128> = HashMap::with_capacity(expected_collision_at * 3 / 2);
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

fn check_hash(item: WorkerResult, hashes: &mut HashMap<u128, u128>) -> Option<SaylerResult> {
    if hashes.contains_key(&item.result) {
        let previous_input = *hashes.get(&item.result).unwrap();
        return Some(SaylerResult {inputs: (previous_input, item.input)});
    }

    hashes.insert(item.result, item.input);
    return None;
}


fn extract_sayler_value_6(hash: Digest) -> u128 {
    return u128::from_le_bytes([hash[0], hash[1], hash[2], hash[13], hash[14], hash[15], 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
}

fn spawn_workers_6(threads: usize, reader_txs: Vec<mpsc::SyncSender<WorkerResult>>, stride: u128, partitions: usize) {
    eprintln!("Spawning workers optimized for finding Sayler 6-collisions");

    let worker_pool = rayon::ThreadPoolBuilder::new().num_threads(threads);
    worker_pool.build().unwrap().install(|| {
        (0..std::u128::MAX).into_par_iter()
            .for_each_with(WorkerInit {reader_txs, stride, partitions, n: 6}, hash_worker_6);
    });
}

fn hash_worker_6(init: &mut WorkerInit, input: u128) {
    let digest = md5::compute(input.to_be_bytes());
    let sayler_value = extract_sayler_value_6(digest);

    let reader = partition(sayler_value, init.stride, init.partitions);
    let tx = init.reader_txs[reader].borrow();
    match tx.send(WorkerResult { input, result: sayler_value }) {
        Err(e) => eprintln!("Worker error while sending result to reader {}: {}", reader, e),
        Ok(_) => (),
    }
}

fn extract_sayler_value_10(hash: Digest) -> u128 {
    return u128::from_le_bytes([hash[0], hash[1], hash[2], hash[3], hash[4], hash[11], hash[12], hash[13], hash[14], hash[15], 0, 0, 0, 0, 0, 0]);
}

fn spawn_workers_10(threads: usize, reader_txs: Vec<mpsc::SyncSender<WorkerResult>>, stride: u128, partitions: usize) {
    eprintln!("Spawning workers optimized for finding Sayler 10-collisions");

    let worker_pool = rayon::ThreadPoolBuilder::new().num_threads(threads);
    worker_pool.build().unwrap().install(|| {
        (0..std::u128::MAX).into_par_iter()
            .for_each_with(WorkerInit {reader_txs, stride, partitions, n: 10}, hash_worker_10);
    });
}

fn hash_worker_10(init: &mut WorkerInit, input: u128) {
    let digest = md5::compute(input.to_be_bytes());
    let sayler_value = extract_sayler_value_10(digest);

    let reader = partition(sayler_value, init.stride, init.partitions);
    let tx = init.reader_txs[reader].borrow();
    match tx.send(WorkerResult { input, result: sayler_value }) {
        Err(e) => eprintln!("Worker error while sending result to reader {}: {}", reader, e),
        Ok(_) => (),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    mod test_partition {
        use super::*;

        #[test]
        fn test_partition() {
            let values = 0..6;
            let expected_partitions = (0..2).flat_map(|i: usize| vec![i, i].into_iter());
            let stride = 2;
            let partitions = 3;

            values.zip(expected_partitions).for_each(|(value, expected_partition)| {
                let actual = partition(value, stride, partitions);
                assert_eq!(actual, expected_partition,
                           "Mismatch between actual and expected partitions for input value {}", value);
            });
        }

        #[test]
        fn test_extra_values_fall_into_last_partition() {
            // Given 3 partitions and a range of values from 0 to 6 inclusive, the partitions are
            // expected to look like: | 0, 1 | 2, 3 | 4, 5, 6 |
            let value = 6;
            let expected_partition = 2;
            let stride = 2;
            let partitions = 3;


            let actual = partition(value, stride, partitions);
            assert_eq!(actual, expected_partition);
        }
    }

    mod test_extract_sayler_value {
        use super::*;

        #[test]
        fn test_extract_sayler_value() {
            let a = md5::Digest([0, 1, 2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 12, 13, 14, 15]);
            let b = md5::Digest([0, 1, 2, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 13, 14, 15]);
            let c = md5::Digest([99; 16]);
            let d = md5::Digest([1, 0, 2, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 13, 14, 15]);

            // use the first 3 bytes (6 hex values) and last 3 bytes (6 hex values)
            let n = 6;

            let sayler_a = extract_sayler_value(a, n);
            let sayler_b = extract_sayler_value(b, n);
            let sayler_c = extract_sayler_value(c, n);
            let sayler_d = extract_sayler_value(d, n);

            assert_eq!(sayler_a, sayler_b);
            assert_ne!(sayler_a, sayler_c);
            assert_ne!(sayler_a, sayler_d);
        }
    }
}
