# Embassy ring-buffer test fixture

`embassy_ringbuffer.rs` is an unmodified copy of:

https://github.com/embassy-rs/embassy/blob/b2c49cceb6a3c600494347ebd5c0cdb6a163f04a/embassy-stm32/src/dma/ringbuffer.rs

This matches the Embassy revision in the project's lockfile. It is included only
in library tests, with a minimal `Word` trait supplied by `src/tests/dma.rs`.
No vendored code is linked into the firmware. Retain the upstream source and
its MIT license together; do not apply local formatting or algorithm changes.

Run it with the other host tests:

```sh
cargo test --locked --lib --target aarch64-apple-darwin
```

When updating Embassy, update this fixture from the same revision and rerun both
the startup regression tests and the firmware cross-build. These tests exercise
ring bookkeeping with fake DMA positions, not STM32 peripheral timing.
