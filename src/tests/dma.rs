//! Host-only probe of Embassy's unmodified ringbuffer.rs at b2c49cce.
//! This exercises real ring bookkeeping with fake DMA positions, not STM32 hardware.

pub mod word {
    pub trait Word: Copy {}
    impl Word for u16 {}
    impl Word for u32 {}
}

// Preserve the pinned source, including upstream-only feature gates and formatting.
#[rustfmt::skip]
#[allow(dead_code, unexpected_cfgs)]
#[path = "upstream/embassy_ringbuffer.rs"]
mod ringbuffer;

#[cfg(test)]
mod startup {
    use super::ringbuffer::{DmaCtrl, Error, WritableDmaRingBuffer};
    use core::cell::Cell;
    use core::future::Future;
    use core::pin::pin;
    use core::task::{Context, Poll, Waker};

    const WORDS: usize = 2048;

    struct Dma<'a> {
        position: &'a Cell<usize>,
    }

    impl DmaCtrl for Dma<'_> {
        fn get_remaining_transfers(&self) -> usize {
            WORDS - self.position.get()
        }

        fn reset_complete_count(&mut self) -> usize {
            0
        }

        fn set_waker(&mut self, _: &Waker) {}
    }

    #[test]
    fn zeroed_memory_alone_does_not_prime_the_ring() {
        let mut storage = [0u16; WORDS];
        let mut ring = WritableDmaRingBuffer::new(&mut storage);
        let position = Cell::new(80);
        let mut dma = Dma {
            position: &position,
        };
        assert_eq!(ring.write(&mut dma, &[0; 512]), Err(Error::Overrun));
    }

    #[test]
    fn explicit_silence_prefill_survives_codec_start_and_first_tone_write() {
        let mut storage = [0u16; WORDS];
        let mut ring = WritableDmaRingBuffer::new(&mut storage);
        let position = Cell::new(0);
        let mut dma = Dma {
            position: &position,
        };
        for block in 0..4 {
            assert_eq!(
                ring.write(&mut dma, &[0; 512]),
                Ok((512, WORDS - (block + 1) * 512))
            );
        }

        // About 0.8 ms of DMA progress during three 100 kHz I2C writes.
        position.set(80);
        let tone = [1u16; 512];
        let mut context = Context::from_waker(Waker::noop());
        let mut write = pin!(ring.write_exact(&mut dma, &tone));
        assert_eq!(write.as_mut().poll(&mut context), Poll::Pending);
        // Resume the same future at the first half-transfer interrupt.
        position.set(WORDS / 2);
        assert_eq!(write.as_mut().poll(&mut context), Poll::Ready(Ok(512)));
    }
}
