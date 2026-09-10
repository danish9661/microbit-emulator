use crate::system::System;
use super::Peripheral;

/// NVMC @ 0x4001E000 (Non-Volatile Memory Controller).
/// P1 stub: READY=1 always, ERASE/Page + WRITE enable handshake accepted,
/// writes to flash go through the normal memory path (no JS driver needed
/// for bring-up). Unlisted offsets read-as-0.
pub struct Nvmc {
    pub config: u32,
    pub erasepage: u32,
    ready: bool,
}

impl Default for Nvmc {
    fn default() -> Self {
        Self { config: 0, erasepage: 0, ready: true }
    }
}

impl Nvmc {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "NVMC" { Some(Box::new(Self::default())) } else { None }
    }
}

impl Peripheral for Nvmc {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x504 => self.config,       // CONFIG (REN/WEN/EEN)
            0x508 => self.erasepage,    // ERASEPAGE
            0x400 => self.ready as u32, // READY
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x504 => self.config = value & 0x3,
            0x508 => self.erasepage = value, // JS/native harness applies erase
            0x400 => {} // READY read-only
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn ready_and_config() {
        let sys = test_dummy_system();
        let mut n = Nvmc::default();
        assert_eq!(n.read(&sys, 0x400), 1);
        n.write(&sys, 0x504, 1);
        assert_eq!(n.read(&sys, 0x504), 1);
    }
}
