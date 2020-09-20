use std::collections::HashMap;
use md5;
use std::time;

/// Finds a Sayler n-Collision for MD5
///
/// A Sayler n-Collision is a pair of distinct inputs whose md5sum matches in the first n and last
/// n printed hex characters. For example this is a Sayler-6 Collision:
///
/// $ md5sum file1
/// d41d8ce1987fbb152380234511f8427e  file1
/// $ md5sum file2
/// d41d8cd98f00b204e9800998ecf8427e  file2
pub fn find_collision(n: u8) -> Result<(u128, u128), &'static str> {
    if n > 16 {
        return Err("There are only 32 hex digits in an MD5 hash, therefore there are no collisions for n > 16");
    }

    // TODO: handle non-even n which would require tracking arrays of 4 bits rather than the arrays
    //  of bytes that the MD5 digest produces
    if n % 2 != 0 {
        return Err("Only even n values are currently supported")
    }

    return single_pass_find_collision(n);
}

/// Uses a hashmap to keep track of the inputs that caused each output hash
///
/// Expected to need ~2^(4*n) entries since a collision is expected after ~2^(4*n) inputs as per
/// the birthday paradox.
fn single_pass_find_collision(n: u8) -> Result<(u128, u128), &'static str> {
    const TIMED_ITERATIONS: u32 = 1000000;

    let n_bytes: usize = (n / 2) as usize;
    let tail_index: usize = (128 / 8) - n_bytes;

    // We expect a collision after roughly 2^(4*n) inputs due to the birthday paradox
    let expected_collision_at: usize = 2 ^ (4 * (n as usize));

    // We pre-allocate a hashmap with ~.5x extra headroom to avoid needing to resize the heap
    let mut hashes: HashMap<Vec<u8>, u128> = HashMap::with_capacity(expected_collision_at * 3 / 2);

    let start = time::Instant::now();
    let mut loop_start = time::Instant::now();
    for input in 0..std::u128::MAX {
        if input % (TIMED_ITERATIONS as u128) == 0 {
            let iter_rate = (TIMED_ITERATIONS as f32) / loop_start.elapsed().as_secs_f32();
            eprintln!("Reached {} iterations, running {} iterations / s", input, iter_rate);
            loop_start = time::Instant::now();
        }

        let digest = md5::compute(input.to_be_bytes());

        // Note: ideally we should be able to use static arrays, however support for generic
        //  constants is needed to do this (implementation in progress at
        //  https://github.com/rust-lang/rust/issues/44580)
        let mut entry: Vec<u8> = Vec::with_capacity(n as usize);
        // Since we're looking for Sayler collisions, we only care about the first and last n/2 bytes
        entry.extend_from_slice(&digest[0..n_bytes]);
        entry.extend_from_slice(&digest[tail_index..]);

        if hashes.contains_key(entry.as_slice()) {
            eprintln!("Completed running {} iterations in {} seconds",
                      input,
                      start.elapsed().as_secs_f32());
            return Ok((*hashes.get(entry.as_slice()).unwrap(), input));
        }
        hashes.insert(entry, input);
    }

    return Err("No collisions found")
}


/// Optimized implementation to find a Sayler 6-collision
///
/// Uses stack-allocated arrays
pub fn find_6_collision() -> Result<(u128, u128), &'static str> {
    const TIMED_ITERATIONS: u32 = 1000000;

    // We expect a collision after roughly 2^(4*n) inputs due to the birthday paradox
    let expected_collision_at: usize = 2 ^ (4 * 6);

    // We pre-allocate a hashmap with ~.5x extra headroom to avoid needing to resize the heap
    let mut hashes: HashMap<[u8; 6], u128> = HashMap::with_capacity(expected_collision_at * 3 / 2);

    let start = time::Instant::now();
    let mut loop_start = time::Instant::now();
    for input in 0..std::u128::MAX {
        if input % (TIMED_ITERATIONS as u128) == 0 {
            let iter_rate = (TIMED_ITERATIONS as f32) / loop_start.elapsed().as_secs_f32();
            eprintln!("Reached {} iterations, running {} iterations / s", input, iter_rate);
            loop_start = time::Instant::now();
        }

        let digest = md5::compute(input.to_be_bytes());

        let entry = [digest[0], digest[1], digest[2], digest[13], digest[14], digest[15]];

        if hashes.contains_key(&entry) {
            eprintln!("Completed running {} iterations in {} seconds",
                      input,
                      start.elapsed().as_secs_f32());
            return Ok((*hashes.get(&entry).unwrap(), input));
        }
        hashes.insert(entry, input);
    }

    return Err("No collisions found")
}