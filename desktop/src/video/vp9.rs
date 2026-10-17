//! VP9 profile 0 with one visible coded picture per packet. Hidden frames,
//! show-existing and superframes refuse before submission; their output
//! multiplicity needs a separate model. Syntax: WebM VP9 uncompressed header.
use super::bits::Bits;

pub(super) struct Format {
    pub width: u32,
    pub height: u32,
    pub full_range: bool,
    pub bt709: bool,
}
impl Format {
    pub fn check(&self, bytes: &[u8], first: bool) -> Result<(), String> {
        if bytes.is_empty() {
            return Err("empty VP9 packet".into());
        }
        let marker = bytes[bytes.len() - 1];
        if marker & 0xe0 == 0xc0 {
            let index = 2 + usize::from((marker & 7) + 1) * usize::from(((marker >> 3) & 3) + 1);
            if bytes.len() >= index && bytes[bytes.len() - index] == marker {
                return Err("VP9 superframes unsupported".into());
            }
        }
        let mut b = Bits::raw(bytes);
        if b.bits(2)? != 2 || b.bits(2)? != 0 {
            return Err("VP9 requires profile zero".into());
        }
        if b.flag()? {
            return Err("VP9 show-existing pictures unsupported".into());
        }
        let key = !b.flag()?;
        if !b.flag()? {
            return Err("VP9 hidden pictures unsupported".into());
        }
        let error_resilient = b.flag()?;
        if first && !key {
            return Err("VP9 seek did not land on a key picture".into());
        }
        let (width, height) = if key {
            if b.bits(24)? != 0x498342 {
                return Err("invalid VP9 sync code".into());
            }
            let matrix = b.bits(3)?;
            let full = b.flag()?;
            if !matches!(matrix, 0..=3) || (matrix == 2) != self.bt709 || full != self.full_range {
                return Err("VP9 colour configuration changed or unsupported".into());
            }
            (b.bits(16)? + 1, b.bits(16)? + 1)
        } else {
            if !error_resilient {
                b.bits(2)?;
            }
            b.bits(8)?; // refresh flags; every reference has the checked geometry.
            for _ in 0..3 {
                b.bits(3)?;
                b.flag()?;
            }
            let mut from_reference = false;
            for _ in 0..3 {
                if b.flag()? {
                    from_reference = true;
                    break;
                }
            }
            if from_reference {
                (self.width, self.height)
            } else {
                (b.bits(16)? + 1, b.bits(16)? + 1)
            }
        };
        if width != self.width || height != self.height {
            return Err("VP9 geometry changed".into());
        }
        if b.flag()? && (b.bits(16)? + 1 != width || b.bits(16)? + 1 != height) {
            return Err("VP9 render size differs from coded size".into());
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    pub fn configuration(&self) -> Vec<u8> {
        // ISO VP Codec Configuration Box payload, version 1, profile 0,
        // level 5.1, 8-bit 4:2:0. Actual headers remain authoritative.
        vec![
            1,
            0,
            0,
            0,
            0,
            51,
            0x82 | u8::from(self.full_range),
            2,
            2,
            if self.bt709 { 1 } else { 6 },
            0,
            0,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hidden_repeated_and_superframe_packets_refuse_before_decode() {
        let f = Format {
            width: 16,
            height: 16,
            full_range: false,
            bt709: false,
        };
        for packet in [
            &[][..],
            &[0x88],
            &[0x80],
            &[0x82],
            &[0x86],
            &[0x82, 0xc0, 0, 0xc0],
        ] {
            assert!(f.check(packet, true).is_err());
        }
        // A profile-zero key header with one 16x16 visible picture.
        let mut bits = String::from("10000010");
        bits.push_str(&format!("{:024b}", 0x498342));
        bits.push_str("0010");
        bits.push_str(&format!("{:016b}{:016b}", 15, 15));
        bits.push('0');
        while bits.len() % 8 != 0 {
            bits.push('0');
        }
        let bytes: Vec<u8> = bits
            .as_bytes()
            .chunks(8)
            .map(|c| c.iter().fold(0, |v, b| (v << 1) | (b - b'0')))
            .collect();
        assert!(f.check(&bytes, true).is_ok());
        assert!(Format { width: 32, ..f }.check(&bytes, true).is_err());
    }
}
