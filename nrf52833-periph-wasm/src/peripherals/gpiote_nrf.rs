use crate::system::System;
use super::Peripheral;

/// GPIOTE @ 0x40006000 (IRQ 6). P3 subset: 8 channels.
///   TASKS_OUT[n] 0x000+n*4, TASKS_SET[n] 0x030+n*4, TASKS_CLR[n] 0x060+n*4,
///   EVENTS_IN[n] 0x100+n*4, EVENTS_PORT 0x17C,
///   INTENSET 0x304, INTENCLR 0x308, CONFIG[n] 0x510+n*4.
/// CONFIG: MODE[1:0] (0=disabled,1=event,3=task), PSEL (pin 0-31 + port bit5),
/// POLARITY[17:16] (0=none,1=LoToHi,2=HiToLo,3=toggle), OUTINIT[20].
/// Task writes drive GPIO OUT; gpio_set_input + poll() raises IN events
/// (buttons A/B: P0.14 / P0.23).
pub struct Gpiote {
    config: [u32; 8],
    ev_in: [bool; 8],
    ev_port: bool,
    intenset: u32,
    last_in: [bool; 8],
}

impl Default for Gpiote {
    fn default() -> Self {
        Self { config: [0; 8], ev_in: [false; 8], ev_port: false, intenset: 0, last_in: [true; 8] }
    }
}

impl Gpiote {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "GPIOTE" { Some(Box::new(Self::default())) } else { None }
    }
    fn cfg_pin(cfg: u32) -> Option<(u8, u8)> {
        if cfg & 3 == 0 { return None; }
        let psel = (cfg >> 8) & 0x3F;
        Some(((psel >> 5) as u8, (psel & 0x1F) as u8))
    }
    fn polarity(cfg: u32) -> u32 { (cfg >> 16) & 3 }
    fn fire(&self, sys: &System, bit: u32) {
        if self.intenset & bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(6);
        }
    }
    /// Sample GPIO inputs, raise IN/PORT events on matching edges.
    /// Called from tick() and after every read/write.
    fn poll(&mut self, sys: &System) {
        let gpio = sys.p.gpio.borrow();
        for ch in 0..8 {
            let cfg = self.config[ch];
            let Some((port, pin)) = Self::cfg_pin(cfg) else { continue; };
            if cfg & 3 != 1 { continue; } // event mode only
            let lvl = if port < 2 && pin < 32 {
                ((gpio.input_state[port as usize] >> pin) & 1) == 1
            } else { false };
            let pol = Self::polarity(cfg);
            let edge = match pol {
                1 => !self.last_in[ch] && lvl,
                2 => self.last_in[ch] && !lvl,
                3 => self.last_in[ch] != lvl,
                _ => false,
            };
            self.last_in[ch] = lvl;
            if edge {
                self.ev_in[ch] = true;
                self.ev_port = true;
                self.fire(sys, 1 << ch);
                if self.intenset & (1 << 31) != 0 {
                    sys.p.nvic.borrow_mut().set_intr_pending(6);
                }
            }
        }
    }
    fn drive_task(&self, sys: &System, ch: usize, level: Option<bool>) {
        let cfg = self.config[ch];
        if cfg & 3 != 3 { return; } // task mode only
        // NOTE: no POLARITY gate here. On silicon TASKS_SET/CLR/OUT
        // drive the pin unconditionally in task mode; POLARITY only
        // selects the *event* edge in event mode (SVD: MODE=Task vs
        // Event are disjoint behaviors). A polarity gate here kills
        // the CODAL LED-matrix strobe: NRF52LedMatrix programs
        // CONFIG polarity LoToHi (expected TOGGLE shape) but drives
        // columns via PPI->TASKS_SET, which must set unconditionally
        // (proven: MakeCode park has PPI CH3-5 -> SET1/2/3 armed with
        // CC1/2/3 = 0/0/0 so COMPARE1/2/3 never fire — the model was
        // never reached, but a gated SET would no-op even when fired).
        let Some((port, pin)) = Self::cfg_pin(cfg) else { return; };
        let mut gpio = sys.p.gpio.borrow_mut();
        if port < 2 && pin < 32 {
            let mask = 1u32 << pin;
            match level {
                Some(true) => gpio.out[port as usize] |= mask,
                Some(false) => gpio.out[port as usize] &= !mask,
                None => gpio.out[port as usize] ^= mask, // toggle (OUT task)
            }
        }
    }
}

impl Peripheral for Gpiote {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, sys: &System, offset: u32) -> u32 {
        self.poll(sys);
        match offset {
            0x100..=0x11C => self.ev_in[((offset - 0x100) >> 2) as usize] as u32,
            0x17C => self.ev_port as u32,
            0x304 => self.intenset,
            0x510..=0x52C => self.config[((offset - 0x510) >> 2) as usize],
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000..=0x01C => { let ch = (offset >> 2) as usize; self.drive_task(sys, ch, None); }
            0x030..=0x04C => { let ch = ((offset - 0x030) >> 2) as usize; self.drive_task(sys, ch, Some(true)); }
            0x060..=0x07C => { let ch = ((offset - 0x060) >> 2) as usize; self.drive_task(sys, ch, Some(false)); }
            0x100..=0x11C => if value == 0 { self.ev_in[((offset - 0x100) >> 2) as usize] = false; }
            0x17C => if value == 0 { self.ev_port = false; }
            0x304 => self.intenset |= value,
            0x308 => self.intenset &= !value,
            0x510..=0x52C => self.config[((offset - 0x510) >> 2) as usize] = value,
            _ => {}
        }
        self.poll(sys);
    }
    fn tick(&mut self, sys: &System) { self.poll(sys); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn button_edge_raises_in_event() {
        let sys = test_dummy_system();
        let mut g = Gpiote::default();
        // CH0: event mode, P0.14 (BTN_A), LoToHi
        g.write(&sys, 0x510, (1) | (14 << 8) | (1 << 16));
        sys.p.gpio.borrow_mut().set_input_pin(0, 14, false);
        g.tick(&sys);
        assert_eq!(g.read(&sys, 0x100), 0);
        sys.p.gpio.borrow_mut().set_input_pin(0, 14, true);
        g.tick(&sys);
        assert_eq!(g.read(&sys, 0x100), 1, "IN0 on rising edge");
        assert_eq!(g.read(&sys, 0x17C), 1, "PORT follows");
        g.write(&sys, 0x100, 0);
        assert_eq!(g.read(&sys, 0x100), 0);
    }
    #[test]
    fn task_out_drives_gpio() {
        let sys = test_dummy_system();
        let mut g = Gpiote::default();
        // CH1: task mode, P0.21
        g.write(&sys, 0x514, (3) | (21 << 8));
        g.write(&sys, 0x034, 1); // SET1
        assert!(sys.p.gpio.borrow().read_output_pin(0, 21));
        g.write(&sys, 0x064, 1); // CLR1
        assert!(!sys.p.gpio.borrow().read_output_pin(0, 21));
    }
    #[test]
    fn set_clr_ignore_polarity_in_task_mode() {
        // CODAL LED-matrix strobe programs CONFIG polarity LoToHi
        // but drives columns via PPI->TASKS_SET: SET/CLR must act
        // unconditionally in task mode (POLARITY gates event edges
        // only — SVD MODE=Task vs Event are disjoint).
        let sys = test_dummy_system();
        let mut g = Gpiote::default();
        // CH3: task mode, P0.31, polarity LoToHi (matrix strobe shape)
        g.write(&sys, 0x51C, (3) | (31 << 8) | (1 << 16) | (1 << 20));
        g.write(&sys, 0x03C, 1); // SET3
        assert!(sys.p.gpio.borrow().read_output_pin(0, 31), "SET with LoToHi polarity");
        g.write(&sys, 0x06C, 1); // CLR3
        assert!(!sys.p.gpio.borrow().read_output_pin(0, 31), "CLR with LoToHi polarity");
        // OUT (toggle) likewise ignores polarity.
        g.write(&sys, 0x00C, 1); // OUT3
        assert!(sys.p.gpio.borrow().read_output_pin(0, 31), "OUT toggle ungated");
    }
}
