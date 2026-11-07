// Encodes prompts.txt into an XOR byte pool so the prompts are not readable with `strings`.
// Same keystream idea as sentinel-kit's kernels. This is obfuscation, not secrecy: prompts
// still travel to the provider in plaintext.
use std::{env, fs, path::Path};

fn mix(mut z: u32) -> u32 {
    z = z.wrapping_add(0x9e37_79b9);
    z = (z ^ (z >> 16)).wrapping_mul(0x85eb_ca6b);
    z = (z ^ (z >> 13)).wrapping_mul(0xc2b2_ae35);
    z ^ (z >> 16)
}

fn main() {
    println!("cargo:rerun-if-changed=prompts.txt");
    let text = fs::read_to_string("prompts.txt").expect("prompts.txt");
    let salt = 0x51f2_a9c3u32 ^ env::var("CARGO_PKG_VERSION").unwrap().bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let mut out = format!("pub const SALT: u32 = {salt};\npub const POOL: &[&[u8]] = &[\n");
    for part in text.split("\n%%\n") {
        let enc: Vec<String> = part.trim().bytes().enumerate()
            .map(|(i, b)| (b ^ mix(salt.wrapping_add(i as u32)) as u8).to_string()).collect();
        out += &format!("&[{}],\n", enc.join(","));
    }
    out += "];\n";
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("pool.rs"), out).unwrap();
}
