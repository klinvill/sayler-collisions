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

use md5;

fn main() {
    // let collision = sayler::find_collision(6).unwrap();
    let collision = sayler::find_6_collision().unwrap();
    let hashes = (
        format!("{:x}", md5::compute(collision.0.to_be_bytes())),
        format!("{:x}", md5::compute(collision.1.to_be_bytes()))
    );

    println!("Collision found! {} and {} both produce the hashes {} and {} respectively",
        collision.0, collision.1, hashes.0, hashes.1);
}
