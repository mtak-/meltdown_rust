# Meltdown Rust

This is a proof of concept of the meltdown attack in rust, based on https://github.com/gkaindl/meltdown-poc

This only works on Intel Haswell processors (or later) produced after November 2014, as it uses hardware transactional memory (TSX-RTM). Early Haswell processors had a bug in their TSX implementation resulting in the disabling of the feature.

To run you must set `RUSTFLAGS="-C target-cpu=native"`

You can change `start_addr` to any arbitrary pointer value, and `len` to some length of bytes to read.