// TODO:
//  - Single process sayler n-collision finding
//  - Efficient storage of previously seen prefixes and the payloads that trigger them (maybe a trie?)
//      -- Max storage needed:
//          --- Trie: ~16^32 nodes, each storing a u128 input
//          --- List: 2^128 entries, each storing a u128 input, equivalent to space needed by a trie
//      -- Expected storage needed:
//          --- Trie: ~2^(n*4) + ???
//          --- HashMap: ~2^(n*4+1)
//      -- For a sayler n-collision, we would really need to track at worst 2^(n*4*2) entries
//          --- According to the birthday paradox, we should actually see a collision at ~ 2^(n*4) entries
//      -- For computational efficiency, we probably want to use a hashmap or trie to record the
//          entries. If we care more about memory, we could try to only store flags and just search
//          again once a collision is found to find the other colliding input
//  - Parallel sayler n-collision finding

mod sayler;
mod par_sayler;

use structopt::StructOpt;
use md5;

/// Finds a MD5 Sayler n-collision
///
/// A MD5 Sayler n-collision is a pair of distinct inputs whose MD5 hash matches in the first n and
/// last n printed hex characters. For example this is a Sayler-6 Collision:
///
/// $ md5sum file1
///
/// d41d8ce1987fbb152380234511f8427e  file1
///
/// $ md5sum file2
///
/// d41d8cd98f00b204e9800998ecf8427e  file2
#[derive(Debug, StructOpt)]
struct Cli {
    /// Number of leading and trailing hex values that must match to count as a collision
    n: u8,
    /// Run parallel search for collisions
    #[structopt(short, long)]
    parallel: bool,
}

fn main() {
    let args = Cli::from_args();

    let collision_fn = match args.parallel {
        true => par_sayler::find_collision,
        false => sayler::find_collision,
    };

    let collision = collision_fn(args.n).unwrap().inputs;

    let hashes = (
        format!("{:x}", md5::compute(collision.0.to_be_bytes())),
        format!("{:x}", md5::compute(collision.1.to_be_bytes()))
    );

    println!("Collision found! {} and {} produce the hashes {} and {} respectively",
        collision.0, collision.1, hashes.0, hashes.1);
}
