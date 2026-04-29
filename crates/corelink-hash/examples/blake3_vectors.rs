//! Helper that prints BLAKE3 outputs for chunk-boundary inputs. Used to
//! generate canonical regression vectors hardcoded in tests.

#![allow(missing_docs, clippy::print_stdout, clippy::expect_used)]

fn main() {
    let inputs: Vec<(String, Vec<u8>)> = vec![
        ("1023".into(), vec![0u8; 1023]),
        ("1024".into(), vec![0u8; 1024]),
        ("1025".into(), vec![0u8; 1025]),
        ("2048".into(), vec![0u8; 2048]),
        ("4096".into(), vec![0u8; 4096]),
        ("65536_0xA5".into(), vec![0xA5u8; 65536]),
    ];
    for (label, body) in inputs {
        let h = blake3::hash(&body);
        println!("{}: {}", label, h.to_hex());
    }
}
