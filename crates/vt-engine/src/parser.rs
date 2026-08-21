//! Byte state machine. Holds no grid (ADR 0020 D6, SPEC-VT-PARSER §1).
//!
//! In-tree parser (ADR 0020 D1). C1 policy: ADR 0020 D3 / S-VT #21.

pub(crate) trait Actions {
    fn print(&mut self, c: char);
    fn execute(&mut self, byte: u8);
    fn csi(&mut self, params: &[u16], intermediates: &[u8], ignore: bool, action: char);
    fn esc(&mut self, intermediates: &[u8], byte: u8);
    fn osc(&mut self, _payload: &[u8]) {}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Ground,
    Escape,
    EscapeIntermediate,
    CsiEntry,
    CsiParam,
    CsiIntermediate,
    CsiIgnore,
    OscString,
    OscEscape,
    DcsPassthrough,
    DcsEscape,
    SosPmApc,
    SosEscape,
}

const MAX_PARAMS: usize = 32;
const MAX_INTER: usize = 2;
const MAX_OSC: usize = 1024;
const MAX_UTF8: usize = 4;

pub(crate) struct Parser {
    state: State,
    params: [u16; MAX_PARAMS],
    param_len: usize,
    param_acc: u32,
    intermediates: [u8; MAX_INTER],
    inter_len: usize,
    ignore: bool,
    osc: [u8; MAX_OSC],
    osc_len: usize,
    #[cfg(feature = "mutate")]
    unbounded_osc: Vec<u8>,
    utf8: [u8; MAX_UTF8],
    utf8_len: u8,
    utf8_needed: u8,
    putback: Option<u8>,
}

impl Parser {
    pub(crate) fn is_idle(&self) -> bool {
        self.state == State::Ground && self.utf8_len == 0 && self.putback.is_none()
    }

    pub(crate) fn new() -> Self {
        Self {
            state: State::Ground,
            params: [0; MAX_PARAMS],
            param_len: 0,
            param_acc: 0,
            intermediates: [0; MAX_INTER],
            inter_len: 0,
            ignore: false,
            osc: [0; MAX_OSC],
            osc_len: 0,
            #[cfg(feature = "mutate")]
            unbounded_osc: Vec::new(),
            utf8: [0; MAX_UTF8],
            utf8_len: 0,
            utf8_needed: 0,
            putback: None,
        }
    }

    pub(crate) fn feed(&mut self, bytes: &[u8], actions: &mut impl Actions) {
        let mut i = 0;
        while i < bytes.len() || self.putback.is_some() {
            let b = match self.putback.take() {
                Some(p) => p,
                None => {
                    let x = bytes[i];
                    i += 1;
                    x
                }
            };
            self.step(b, actions);
        }
    }

    fn reprocess(&mut self, b: u8) {
        self.putback = Some(b);
    }

    fn reset_seq(&mut self) {
        self.params = [0; MAX_PARAMS];
        self.param_len = 0;
        self.param_acc = 0;
        self.intermediates = [0; MAX_INTER];
        self.inter_len = 0;
        self.ignore = false;
        self.osc_len = 0;
    }

    fn abort_utf8(&mut self, actions: &mut impl Actions) {
        if self.utf8_needed > 0 {
            self.utf8_len = 0;
            self.utf8_needed = 0;
            actions.print('\u{FFFD}');
        }
    }

    fn step(&mut self, b: u8, actions: &mut impl Actions) {
        if self.utf8_needed > 0 && !matches!(self.state, State::Ground) {
            self.abort_utf8(actions);
        }

        match self.state {
            State::Ground => self.ground(b, actions),
            State::Escape => self.escape(b, actions),
            State::EscapeIntermediate => self.escape_intermediate(b, actions),
            State::CsiEntry => self.csi_entry(b, actions),
            State::CsiParam => self.csi_param(b, actions),
            State::CsiIntermediate => self.csi_intermediate(b, actions),
            State::CsiIgnore => self.csi_ignore(b, actions),
            State::OscString => self.osc_string(b, actions),
            State::OscEscape => self.osc_escape(b, actions),
            State::DcsPassthrough => self.dcs_passthrough(b, actions),
            State::DcsEscape => self.dcs_escape(b, actions),
            State::SosPmApc => self.sos(b),
            State::SosEscape => self.sos_escape(b, actions),
        }
    }

    fn ground(&mut self, b: u8, actions: &mut impl Actions) {
        if self.utf8_needed > 0 {
            if (0x80..=0xbf).contains(&b) {
                self.utf8[self.utf8_len as usize] = b;
                self.utf8_len += 1;
                if self.utf8_len == self.utf8_needed {
                    self.finish_utf8(actions);
                }
                return;
            }
            self.abort_utf8(actions);
            self.reprocess(b);
            return;
        }

        match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => actions.execute(b),
            0x18 | 0x1a => {
                self.reset_seq();
                self.state = State::Ground;
                actions.execute(b);
            }
            0x1b => {
                self.reset_seq();
                self.state = State::Escape;
            }
            0x20..=0x7e => actions.print(char::from(b)),
            0x7f => {}
            0x80..=0x9f => {
                if crate::mutate("c1_as_control") {
                    actions.execute(b);
                } else {
                    actions.print('\u{FFFD}');
                }
            }
            0xa0..=0xbf => actions.print('\u{FFFD}'),
            0xc0 | 0xc1 => actions.print('\u{FFFD}'),
            0xc2..=0xdf => self.start_utf8(b, 2),
            0xe0..=0xef => self.start_utf8(b, 3),
            0xf0..=0xf4 => self.start_utf8(b, 4),
            0xf5..=0xff => actions.print('\u{FFFD}'),
        }
    }

    fn start_utf8(&mut self, lead: u8, needed: u8) {
        self.utf8 = [0; MAX_UTF8];
        self.utf8[0] = lead;
        self.utf8_len = 1;
        self.utf8_needed = needed;
    }

    fn finish_utf8(&mut self, actions: &mut impl Actions) {
        let needed = self.utf8_needed;
        let buf = self.utf8;
        self.utf8_len = 0;
        self.utf8_needed = 0;
        match decode_utf8(&buf[..needed as usize]) {
            Some(c) => {
                let u = c as u32;
                if crate::mutate("c1_as_control") && (0x80..=0x9f).contains(&u) {
                    actions.execute(u as u8);
                } else {
                    actions.print(c);
                }
            }
            None => actions.print('\u{FFFD}'),
        }
    }

    fn escape(&mut self, b: u8, actions: &mut impl Actions) {
        match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => actions.execute(b),
            0x18 | 0x1a => {
                self.state = State::Ground;
                actions.execute(b);
            }
            0x1b => self.reset_seq(),
            0x20..=0x2f => {
                self.push_inter(b);
                self.state = State::EscapeIntermediate;
            }
            0x30..=0x4f | 0x51..=0x57 | 0x59 | 0x5a | 0x5c | 0x60..=0x7e => {
                actions.esc(&self.intermediates[..self.inter_len], b);
                self.state = State::Ground;
                self.reset_seq();
            }
            0x50 => {
                self.reset_seq();
                self.state = State::DcsPassthrough;
            }
            0x58 | 0x5e | 0x5f => {
                self.reset_seq();
                self.state = State::SosPmApc;
            }
            0x5b => {
                self.reset_seq();
                self.state = State::CsiEntry;
            }
            0x5d => {
                self.reset_seq();
                self.state = State::OscString;
            }
            0x7f => {}
            _ => {
                self.state = State::Ground;
                self.reset_seq();
            }
        }
    }

    fn escape_intermediate(&mut self, b: u8, actions: &mut impl Actions) {
        match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => actions.execute(b),
            0x18 | 0x1a => {
                self.state = State::Ground;
                actions.execute(b);
            }
            0x1b => {
                self.reset_seq();
                self.state = State::Escape;
            }
            0x20..=0x2f => self.push_inter(b),
            0x30..=0x7e => {
                actions.esc(&self.intermediates[..self.inter_len], b);
                self.state = State::Ground;
                self.reset_seq();
            }
            _ => {
                self.state = State::Ground;
                self.reset_seq();
            }
        }
    }

    fn csi_entry(&mut self, b: u8, actions: &mut impl Actions) {
        match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => actions.execute(b),
            0x18 | 0x1a => {
                self.state = State::Ground;
                actions.execute(b);
            }
            0x1b => {
                self.reset_seq();
                self.state = State::Escape;
            }
            0x20..=0x2f => {
                self.push_inter(b);
                self.state = State::CsiIntermediate;
            }
            0x30..=0x39 => {
                self.push_digit(b);
                self.state = State::CsiParam;
            }
            0x3a | 0x3b => {
                self.finish_param();
                self.state = State::CsiParam;
            }
            0x3c..=0x3f => {
                self.push_inter(b);
                self.state = State::CsiParam;
            }
            0x40..=0x7e => self.dispatch_csi(b, actions),
            0x7f => {}
            _ => {
                // High byte inside CSI: consume, do not print (csi_high_param).
                self.ignore = true;
                self.state = State::CsiParam;
            }
        }
    }

    fn csi_param(&mut self, b: u8, actions: &mut impl Actions) {
        match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => actions.execute(b),
            0x18 | 0x1a => {
                self.state = State::Ground;
                actions.execute(b);
            }
            0x1b => {
                self.reset_seq();
                self.state = State::Escape;
            }
            0x20..=0x2f => {
                self.finish_param();
                self.push_inter(b);
                self.state = State::CsiIntermediate;
            }
            0x30..=0x39 => self.push_digit(b),
            0x3a | 0x3b => self.finish_param(),
            0x3c..=0x3f => self.state = State::CsiIgnore,
            0x40..=0x7e => {
                self.finish_param();
                self.dispatch_csi(b, actions);
            }
            0x7f => {}
            _ => {
                self.ignore = true;
            }
        }
    }

    fn csi_intermediate(&mut self, b: u8, actions: &mut impl Actions) {
        match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => actions.execute(b),
            0x18 | 0x1a => {
                self.state = State::Ground;
                actions.execute(b);
            }
            0x1b => {
                self.reset_seq();
                self.state = State::Escape;
            }
            0x20..=0x2f => self.push_inter(b),
            0x30..=0x3f => self.state = State::CsiIgnore,
            0x40..=0x7e => self.dispatch_csi(b, actions),
            _ => {
                self.ignore = true;
                self.state = State::CsiIgnore;
            }
        }
    }

    fn csi_ignore(&mut self, b: u8, actions: &mut impl Actions) {
        match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => actions.execute(b),
            0x18 | 0x1a => {
                self.state = State::Ground;
                actions.execute(b);
            }
            0x1b => {
                self.reset_seq();
                self.state = State::Escape;
            }
            0x40..=0x7e => {
                self.state = State::Ground;
                self.reset_seq();
            }
            _ => {}
        }
    }

    fn dispatch_csi(&mut self, action: u8, actions: &mut impl Actions) {
        actions.csi(
            &self.params[..self.param_len],
            &self.intermediates[..self.inter_len],
            self.ignore,
            char::from(action),
        );
        self.state = State::Ground;
        self.reset_seq();
    }

    fn osc_string(&mut self, b: u8, actions: &mut impl Actions) {
        match b {
            0x07 => {
                actions.osc(&self.osc[..self.osc_len]);
                self.state = State::Ground;
                self.reset_seq();
            }
            0x1b => self.state = State::OscEscape,
            0x18 | 0x1a => {
                self.state = State::Ground;
                self.reset_seq();
                actions.execute(b);
            }
            _ => self.push_osc(b),
        }
    }

    fn osc_escape(&mut self, b: u8, actions: &mut impl Actions) {
        if b == b'\\' {
            actions.osc(&self.osc[..self.osc_len]);
            self.state = State::Ground;
            self.reset_seq();
        } else {
            // Abandoned on ESC (SPEC-VT-PARSER §8); the ESC starts a new sequence.
            self.state = State::Escape;
            self.reset_seq();
            self.reprocess(b);
        }
    }

    fn dcs_passthrough(&mut self, b: u8, actions: &mut impl Actions) {
        match b {
            0x1b => self.state = State::DcsEscape,
            0x18 | 0x1a => {
                self.state = State::Ground;
                self.reset_seq();
                actions.execute(b);
            }
            _ => {}
        }
    }

    fn dcs_escape(&mut self, b: u8, _actions: &mut impl Actions) {
        if b == b'\\' {
            self.state = State::Ground;
            self.reset_seq();
        } else {
            self.state = State::Escape;
            self.reset_seq();
            self.reprocess(b);
        }
    }

    fn sos(&mut self, b: u8) {
        match b {
            0x1b => self.state = State::SosEscape,
            0x18 | 0x1a => {
                self.state = State::Ground;
                self.reset_seq();
            }
            _ => {}
        }
    }

    fn sos_escape(&mut self, b: u8, _actions: &mut impl Actions) {
        if b == b'\\' {
            self.state = State::Ground;
            self.reset_seq();
        } else {
            self.state = State::Escape;
            self.reset_seq();
            self.reprocess(b);
        }
    }

    fn push_inter(&mut self, b: u8) {
        if self.inter_len < MAX_INTER {
            self.intermediates[self.inter_len] = b;
            self.inter_len += 1;
        } else {
            self.ignore = true;
        }
    }

    fn push_digit(&mut self, b: u8) {
        let d = u32::from(b - b'0');
        self.param_acc = self.param_acc.saturating_mul(10).saturating_add(d);
        if self.param_acc > u32::from(u16::MAX) {
            self.param_acc = u32::from(u16::MAX);
        }
    }

    fn finish_param(&mut self) {
        if self.param_len < MAX_PARAMS {
            self.params[self.param_len] = self.param_acc.min(u32::from(u16::MAX)) as u16;
            self.param_len += 1;
        } else {
            self.ignore = true;
        }
        self.param_acc = 0;
    }

    fn push_osc(&mut self, b: u8) {
        if crate::mutate("unbounded_osc") {
            #[cfg(feature = "mutate")]
            self.unbounded_osc.push(b);
            #[cfg(not(feature = "mutate"))]
            let _ = b;
            return;
        }
        if self.osc_len < MAX_OSC {
            self.osc[self.osc_len] = b;
            self.osc_len += 1;
        } else {
            self.ignore = true;
        }
    }
}

fn decode_utf8(bytes: &[u8]) -> Option<char> {
    std::str::from_utf8(bytes).ok()?.chars().next()
}
