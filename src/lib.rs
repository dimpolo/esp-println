#![no_std]

#[cfg(feature = "log")]
pub mod logger;
#[cfg(feature = "rtt")]
mod rtt;

#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {{
        #[cfg(not(feature = "no-op"))]
        {
            use core::fmt::Write;
            writeln!($crate::Printer, $($arg)*).ok();
        }
    }};
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        #[cfg(not(feature = "no-op"))]
        {
            use core::fmt::Write;
            write!($crate::Printer, $($arg)*).ok();
        }
    }};
}

pub struct Printer;

impl core::fmt::Write for Printer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        Printer.write_bytes(s.as_bytes());
        Ok(())
    }
}

#[cfg(feature = "rtt")]
mod rtt_printer {
    impl super::Printer {
        pub fn write_bytes(&mut self, bytes: &[u8]) {
            super::with(|| {
                let count = crate::rtt::write_bytes_internal(bytes);
                if count < bytes.len() {
                    crate::rtt::write_bytes_internal(&bytes[count..]);
                }
            })
        }
    }
}

#[cfg(feature = "jtag_serial")]
mod serial_jtag_printer {
    #[cfg(feature = "esp32c3")]
    const SERIAL_JTAG_FIFO_REG: usize = 0x6004_3000;
    #[cfg(feature = "esp32c3")]
    const SERIAL_JTAG_CONF_REG: usize = 0x6004_3004;

    #[cfg(any(feature = "esp32c6", feature = "esp32h2"))]
    const SERIAL_JTAG_FIFO_REG: usize = 0x6000_F000;
    #[cfg(any(feature = "esp32c6", feature = "esp32h2"))]
    const SERIAL_JTAG_CONF_REG: usize = 0x6000_F004;

    #[cfg(feature = "esp32s3")]
    const SERIAL_JTAG_FIFO_REG: usize = 0x6003_8000;
    #[cfg(feature = "esp32s3")]
    const SERIAL_JTAG_CONF_REG: usize = 0x6003_8004;

    /// TODO
    const WAIT_CYCLES: u32 = 100_000;

    #[cfg(any(
        feature = "esp32c3",
        feature = "esp32c6",
        feature = "esp32h2",
        feature = "esp32s3"
    ))]
    impl super::Printer {
        pub fn write_bytes(&mut self, bytes: &[u8]) {
            super::with(|| {
                let fifo = SERIAL_JTAG_FIFO_REG as *mut u32;
                let conf = SERIAL_JTAG_CONF_REG as *mut u32;

                unsafe {
                    if conf.read_volatile() & 0b011 == 0b000 {
                        // FIFO full, should only happen when USB disconnected
                        return;
                    }

                    // todo 64 byte chunks max
                    'outer: for chunk in bytes.chunks(64) {
                        for &b in chunk {
                            fifo.write_volatile(b as u32);
                        }
                        conf.write_volatile(0b001);

                        for _ in 0..WAIT_CYCLES {
                            if conf.read_volatile() & 0b011 != 0b000 {
                                continue 'outer;
                            }
                        }
                        return; // timeout
                    }
                }
            })
        }
    }
}

#[cfg(feature = "uart")]
mod uart_printer {
    #[cfg(feature = "esp32")]
    const UART_TX_ONE_CHAR: usize = 0x4000_9200;
    #[cfg(any(feature = "esp32c2", feature = "esp32c6", feature = "esp32h2"))]
    const UART_TX_ONE_CHAR: usize = 0x4000_0058;
    #[cfg(feature = "esp32c3")]
    const UART_TX_ONE_CHAR: usize = 0x4000_0068;
    #[cfg(feature = "esp32s3")]
    const UART_TX_ONE_CHAR: usize = 0x4000_0648;
    #[cfg(feature = "esp8266")]
    const UART_TX_ONE_CHAR: usize = 0x4000_3b30;

    impl super::Printer {
        #[cfg(not(feature = "esp32s2"))]
        pub fn write_bytes(&mut self, bytes: &[u8]) {
            super::with(|| {
                for &b in bytes {
                    unsafe {
                        let uart_tx_one_char: unsafe extern "C" fn(u8) -> i32 =
                            core::mem::transmute(UART_TX_ONE_CHAR);
                        uart_tx_one_char(b)
                    };
                }
            })
        }

        #[cfg(feature = "esp32s2")]
        pub fn write_bytes(&mut self, bytes: &[u8]) {
            super::with(|| {
                // On ESP32-S2 the UART_TX_ONE_CHAR ROM-function seems to have some issues.
                for chunk in bytes.chunks(64) {
                    for &b in chunk {
                        unsafe {
                            // write FIFO
                            (0x3f400000 as *mut u32).write_volatile(b as u32);
                        };
                    }

                    // wait for TX_DONE
                    while unsafe { (0x3f400004 as *const u32).read_volatile() } & (1 << 14) == 0 {}
                    unsafe {
                        // reset TX_DONE
                        (0x3f400010 as *mut u32).write_volatile(1 << 14);
                    }
                }
            })
        }
    }
}

#[inline]
fn with<R>(f: impl FnOnce() -> R) -> R {
    #[cfg(feature = "critical-section")]
    return critical_section::with(|_| f());

    #[cfg(not(feature = "critical-section"))]
    f()
}
