# Meltdown Rust

This is a proof of concept of the meltdown attack in rust, based on https://github.com/gkaindl/meltdown-poc

This only works on intel haswell processors or later, as it uses hardware transactional memory.

To run you must set `RUSTFLAGS="-C target-cpu=native"`

You can change `start_addr` to any arbitrary pointer value, and `len` to some length of bytes to read.