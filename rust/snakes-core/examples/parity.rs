// SPDX-License-Identifier: GPL-3.0-or-later
use std::io::{self, Read};
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    snakes_core::parity::run(&input, &mut io::BufWriter::new(io::stdout())).unwrap();
}
