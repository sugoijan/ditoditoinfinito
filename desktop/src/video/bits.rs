//! Bounded, borrowing header reads; compressed payloads are never copied.
pub(super) struct Bits<'a> {
    bytes: &'a [u8],
    at: usize,
    byte: u8,
    left: u32,
    zeros: u8,
    escaped: bool,
}
impl<'a> Bits<'a> {
    pub fn raw(bytes: &'a [u8]) -> Self {
        Self {
            bytes: &bytes[..bytes.len().min(65536)],
            at: 0,
            byte: 0,
            left: 0,
            zeros: 0,
            escaped: false,
        }
    }
    pub fn rbsp(bytes: &'a [u8]) -> Self {
        Self {
            escaped: true,
            ..Self::raw(bytes)
        }
    }
    fn octet(&mut self) -> Result<u8, String> {
        loop {
            let b = *self.bytes.get(self.at).ok_or("truncated video header")?;
            self.at += 1;
            if self.escaped && self.zeros >= 2 && b == 3 {
                self.zeros = 0;
                continue;
            }
            self.zeros = if b == 0 {
                self.zeros.saturating_add(1)
            } else {
                0
            };
            return Ok(b);
        }
    }
    pub fn bits(&mut self, n: u32) -> Result<u32, String> {
        if n > 32 {
            return Err("oversized bit read".into());
        }
        let mut value = 0;
        for _ in 0..n {
            if self.left == 0 {
                self.byte = self.octet()?;
                self.left = 8;
            }
            self.left -= 1;
            value = (value << 1) | u32::from((self.byte >> self.left) & 1);
        }
        Ok(value)
    }
    pub fn flag(&mut self) -> Result<bool, String> {
        Ok(self.bits(1)? != 0)
    }
    pub fn ue(&mut self, max: u32) -> Result<u32, String> {
        let mut zeros = 0;
        while !self.flag()? {
            zeros += 1;
            if zeros > 30 {
                return Err("oversized video Exp-Golomb value".into());
            }
        }
        let n = (1u32 << zeros) - 1 + self.bits(zeros)?;
        if n > max {
            return Err("video header value exceeds supported bound".into());
        }
        Ok(n)
    }
}
