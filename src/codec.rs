//! Minimal CS43L22 control for headphone-only, slave Philips I2S playback.
//!
//! Based on Cirrus DS792F2 sections 4.9, 4.11, and 7. Reset must be released
//! before configure(); start() requires stable I2S clocks and a silence stream.

const ADDRESS: u8 = 0x4a; // Seven-bit address, not ST HAL's shifted 0x94.
const POWER_CONTROL_1: u8 = 0x02;
const MISC_CONTROL: u8 = 0x0e;
const PLAYBACK_CONTROL_2: u8 = 0x0f;
const MASTER_MINUS_30_DB: u8 = 0xc4; // Signed -60 steps of 0.5 dB.

/// The two control transactions used by the codec, separated from the MCU HAL
/// so the exact address, wire bytes, and failures can be tested on the host.
pub trait ControlBus {
    type Error;

    fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), Self::Error>;
    fn write_read(
        &mut self,
        address: u8,
        bytes: &[u8],
        result: &mut [u8],
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error<E> {
    Bus(E),
    UnexpectedId(u8),
}

pub struct Cs43l22<B> {
    bus: B,
}

impl<B: ControlBus> Cs43l22<B> {
    /// Identify and configure the codec, leaving it powered down and muted.
    pub fn configure(bus: B) -> Result<Self, Error<B::Error>> {
        let mut codec = Self { bus };
        let id = codec.read(0x01).map_err(Error::Bus)?;
        if id & 0xf8 != 0xe0 {
            return Err(Error::UnexpectedId(id));
        }

        for (register, value) in [
            (POWER_CONTROL_1, 0x01),
            (MISC_CONTROL, 0x00), // Passthrough/de-emphasis off; immediate setup volume updates.
            (PLAYBACK_CONTROL_2, 0xf0), // Mute headphones and speakers during setup.
            (0x04, 0xaf),         // Headphones always on, speakers always off.
            (0x05, 0x80),         // Auto clock detect; no divide-by-two for 256x MCLK.
            (0x06, 0x07),         // Slave, non-inverted Philips I2S, AWL=11.
            (0x07, 0x00),         // Interface control 2 defaults.
            (0x1a, 0x00),         // PCM A unity gain.
            (0x1b, 0x00),         // PCM B unity gain.
            (0x20, MASTER_MINUS_30_DB),
            (0x21, MASTER_MINUS_30_DB),
            (0x22, 0x00), // Headphone A unity gain.
            (0x23, 0x00), // Headphone B unity gain.
        ] {
            codec.write(register, value).map_err(Error::Bus)?;
        }

        // Required initialization from DS792F2 section 4.11. Preserve reserved
        // defaults in 0x32: blindly writing 0x80/0x00 would corrupt other bits.
        codec.write(0x00, 0x99).map_err(Error::Bus)?;
        codec.write(0x47, 0x80).map_err(Error::Bus)?;
        let value = codec.read(0x32).map_err(Error::Bus)?;
        codec.write(0x32, value | 0x80).map_err(Error::Bus)?;
        codec.write(0x32, value & !0x80).map_err(Error::Bus)?;
        codec.write(0x00, 0x00).map_err(Error::Bus)?;
        Ok(codec)
    }

    /// Call only after MCLK/BCLK/LRCK are running, while transmitting silence.
    pub fn start(&mut self) -> Result<(), B::Error> {
        self.write(POWER_CONTROL_1, 0x9e)?;
        self.write(MISC_CONTROL, 0x02)?; // Enable digital soft ramp.
        self.write(PLAYBACK_CONTROL_2, 0x30) // Unmute headphones; keep speakers muted.
    }

    fn write(&mut self, register: u8, value: u8) -> Result<(), B::Error> {
        self.bus.write(ADDRESS, &[register, value])
    }

    fn read(&mut self, register: u8) -> Result<u8, B::Error> {
        let mut result = [0];
        self.bus.write_read(ADDRESS, &[register], &mut result)?;
        Ok(result[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::vec::Vec;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Transaction {
        Read(u8, u8),
        Write(u8, u8),
    }

    #[derive(Debug, PartialEq, Eq)]
    struct Nack;

    struct MockBus {
        pending: VecDeque<Transaction>,
        completed: usize,
        fail_at: Option<usize>,
    }

    impl MockBus {
        fn new(transactions: Vec<Transaction>) -> Self {
            Self {
                pending: transactions.into(),
                completed: 0,
                fail_at: None,
            }
        }

        fn next(&mut self) -> Result<Transaction, Nack> {
            if self.fail_at == Some(self.completed) {
                return Err(Nack);
            }
            self.completed += 1;
            Ok(self.pending.pop_front().expect("unexpected transaction"))
        }
    }

    impl ControlBus for MockBus {
        type Error = Nack;

        fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), Nack> {
            assert_eq!(address, 0x4a);
            match self.next()? {
                Transaction::Write(register, value) => assert_eq!(bytes, [register, value]),
                transaction => panic!("expected read, got write: {transaction:?}"),
            }
            Ok(())
        }

        fn write_read(&mut self, address: u8, bytes: &[u8], result: &mut [u8]) -> Result<(), Nack> {
            assert_eq!(address, 0x4a);
            match self.next()? {
                Transaction::Read(register, value) => {
                    assert_eq!(bytes, [register]);
                    assert_eq!(result.len(), 1);
                    result[0] = value;
                }
                transaction => panic!("expected write, got read: {transaction:?}"),
            }
            Ok(())
        }
    }

    fn configuration(id: u8, reserved: u8) -> Vec<Transaction> {
        use Transaction::{Read, Write};
        std::vec![
            Read(0x01, id),
            Write(0x02, 0x01),
            Write(0x0e, 0x00),
            Write(0x0f, 0xf0),
            Write(0x04, 0xaf),
            Write(0x05, 0x80),
            Write(0x06, 0x07),
            Write(0x07, 0x00),
            Write(0x1a, 0x00),
            Write(0x1b, 0x00),
            Write(0x20, 0xc4),
            Write(0x21, 0xc4),
            Write(0x22, 0x00),
            Write(0x23, 0x00),
            Write(0x00, 0x99),
            Write(0x47, 0x80),
            Read(0x32, reserved),
            Write(0x32, reserved | 0x80),
            Write(0x32, reserved & 0x7f),
            Write(0x00, 0x00),
        ]
    }

    #[test]
    fn setup_stays_powered_down_until_explicit_start() {
        let mut transactions = configuration(0xe3, 0x3b);
        transactions.extend([
            Transaction::Write(0x02, 0x9e),
            Transaction::Write(0x0e, 0x02),
            Transaction::Write(0x0f, 0x30),
        ]);
        let mut codec = Cs43l22::configure(MockBus::new(transactions)).unwrap();
        assert_eq!(codec.bus.completed, 20);
        assert_eq!(codec.bus.pending.len(), 3);
        codec.start().unwrap();
        assert!(codec.bus.pending.is_empty());
    }

    #[test]
    fn accepts_revision_bits_and_preserves_reserved_register_bits() {
        for revision in 0..8 {
            for reserved in [0x00, 0x3b, 0xbb, 0x7f, 0xff] {
                let codec =
                    Cs43l22::configure(MockBus::new(configuration(0xe0 | revision, reserved)))
                        .unwrap();
                assert!(codec.bus.pending.is_empty());
            }
        }
    }

    #[test]
    fn wrong_id_returns_no_configured_codec_and_performs_no_writes() {
        let bus = MockBus::new(std::vec![Transaction::Read(0x01, 0x00)]);
        assert!(matches!(
            Cs43l22::configure(bus),
            Err(Error::UnexpectedId(0x00))
        ));
    }

    #[test]
    fn every_setup_transaction_propagates_bus_failure() {
        for fail_at in 0..configuration(0xe3, 0x3b).len() {
            let mut bus = MockBus::new(configuration(0xe3, 0x3b));
            bus.fail_at = Some(fail_at);
            assert!(matches!(Cs43l22::configure(bus), Err(Error::Bus(Nack))));
        }
    }

    #[test]
    fn start_stops_at_first_bus_failure() {
        for fail_at in 0..3 {
            let mut codec = Cs43l22::configure(MockBus::new(configuration(0xe3, 0x3b))).unwrap();
            codec.bus.pending.extend([
                Transaction::Write(0x02, 0x9e),
                Transaction::Write(0x0e, 0x02),
                Transaction::Write(0x0f, 0x30),
            ]);
            codec.bus.fail_at = Some(codec.bus.completed + fail_at);
            assert_eq!(codec.start(), Err(Nack));
            assert_eq!(codec.bus.pending.len(), 3 - fail_at);
        }
    }
}
