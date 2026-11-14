//! WAV files written: 16-bit PCM, what every browser, our decoders and
//! StepMania read. The announcer studio writes its tidied takes so
//! (`docs/plans/announcer-studio.md`, decision 6). The writer and its
//! reader are the platform's ([`ddi_platform::wav`]).

pub use ddi_platform::wav::{pcm16_len as len, write_pcm16 as write};
