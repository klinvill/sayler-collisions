mod par_sayler;
mod sayler;

use md5;
use structopt::StructOpt;

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
        format!("{:x}", md5::compute(collision.1.to_be_bytes())),
    );

    println!(
        "Collision found! {} and {} produce the hashes {} and {} respectively",
        collision.0, collision.1, hashes.0, hashes.1
    );
}
