//! What the synthetic plugins share: generated songs (a small simfile and
//! a quarter second of silence), and a stored zip of files. No song here
//! is anyone's: everything is made by code.

/// The simfile of generated song `n`: a dance-single chart (and a
/// dance-double one for even `n`), its meter from `n`.
pub fn simfile(n: u32) -> String {
    let meter = meter(n);
    let mut s = format!(
        "#VERSION:0.83;\n#TITLE:Generated Song {n};\n#ARTIST:Test Artist {};\n#MUSIC:song.wav;\n#OFFSET:0;\n#BPMS:0=120;\n",
        n % 4
    );
    let single = "1000\n0100\n0010\n0001\n";
    s.push_str(&format!(
        "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n#METER:{meter};\n#NOTES:\n{single};\n"
    ));
    if n.is_multiple_of(2) {
        let double = "10000000\n01000000\n00100000\n00010000\n";
        s.push_str(&format!(
            "#NOTEDATA:;\n#STEPSTYPE:dance-double;\n#DIFFICULTY:Medium;\n#METER:{};\n#NOTES:\n{double};\n",
            meter.saturating_sub(1).max(1)
        ));
    }
    s
}

/// Generated song `n`'s hardest meter.
pub fn meter(n: u32) -> u32 {
    n % 15 + 1
}

/// A quarter second of silence: 8 kHz, mono, 16-bit PCM.
pub fn silence() -> Vec<u8> {
    let samples = 2000u32;
    let data = samples * 2;
    let mut w = Vec::with_capacity(44 + data as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&8000u32.to_le_bytes());
    w.extend_from_slice(&16000u32.to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    w.resize(44 + data as usize, 0);
    w
}

/// Generated song `n`'s files, by their paths under `dir`.
pub fn song_files(dir: &str, n: u32) -> Vec<(String, Vec<u8>)> {
    vec![
        (format!("{dir}/song.ssc"), simfile(n).into_bytes()),
        (format!("{dir}/song.wav"), silence()),
    ]
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A zip of `files`, stored (not compressed).
pub fn zip(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let at = out.len() as u32;
        let crc = crc32(data);
        let header = |sig: u32, central: bool| {
            let mut h = Vec::new();
            h.extend_from_slice(&sig.to_le_bytes());
            if central {
                h.extend_from_slice(&20u16.to_le_bytes());
            }
            h.extend_from_slice(&20u16.to_le_bytes());
            h.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8 names
            h.extend_from_slice(&0u16.to_le_bytes()); // stored
            h.extend_from_slice(&0u32.to_le_bytes()); // time, date
            h.extend_from_slice(&crc.to_le_bytes());
            h.extend_from_slice(&(data.len() as u32).to_le_bytes());
            h.extend_from_slice(&(data.len() as u32).to_le_bytes());
            h.extend_from_slice(&(name.len() as u16).to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes());
            if central {
                h.extend_from_slice(&[0; 6]); // comment, disk, internal attributes
                h.extend_from_slice(&0u32.to_le_bytes());
                h.extend_from_slice(&at.to_le_bytes());
            }
            h.extend_from_slice(name.as_bytes());
            h
        };
        out.extend(header(0x0403_4b50, false));
        out.extend_from_slice(data);
        central.extend(header(0x0201_4b50, true));
    }
    let cd_at = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_at.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}
