//! Offline PCM mixer. Its sample cursor is independent of wall-clock execution.
use ruffle_core::backend::audio::*;
use ruffle_core::tag_utils::SwfSlice;
use serde_json::{Value, json};
use std::ops::{Deref, DerefMut};

pub const RATE: u64 = 44100;

pub struct Recorder {
    mixer: AudioMixer,
    pub iteration: u64,
    pub output_frames: u64,
    starts: Vec<Value>,
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            mixer: AudioMixer::new(2, RATE as u32),
            iteration: 0,
            output_frames: 0,
            starts: Vec::new(),
        }
    }

    fn start_stream(
        &mut self,
        data: SwfSlice,
        head: &swf::SoundStreamHead,
    ) -> Result<SoundInstanceHandle, DecodeError> {
        let handle = self.mixer.start_stream(data.clone(), head)?;
        // This logs successful decoder initialization, not perceived sound onset.
        if self.starts.len() < 64 {
            self.starts.push(
                json!({"iteration":self.iteration,"outputSampleFrame":self.output_frames,
                "sliceStart":data.start,"sliceEnd":data.end,
                "compression":format!("{:?}",head.stream_format.compression),
                "sampleRate":head.stream_format.sample_rate,"stereo":head.stream_format.is_stereo,
                "samplesPerMovieFrame":head.num_samples_per_block,"latencySeek":head.latency_seek}),
            );
        } else {
            tracing::error!("audio stream-start audit limit exhausted");
        }
        Ok(handle)
    }
}

impl Deref for Recorder {
    type Target = AudioMixer;
    fn deref(&self) -> &Self::Target {
        &self.mixer
    }
}
impl DerefMut for Recorder {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.mixer
    }
}

pub struct OfflineAudio {
    pub recorder: Recorder,
}

impl OfflineAudio {
    pub fn new() -> Self {
        Self {
            recorder: Recorder::new(),
        }
    }
    pub fn mix_to(&mut self, target: u64) -> Vec<i16> {
        let frames = target
            .checked_sub(self.recorder.output_frames)
            .expect("audio sample cursor must be monotonic");
        let mut samples = vec![0i16; frames as usize * 2];
        self.recorder.mix(&mut samples);
        self.recorder.output_frames = target;
        samples
    }
    pub fn report(&self) -> Value {
        json!({"sampleRate":RATE,"channels":2,"sampleFrames":self.recorder.output_frames,
            "streamStarts":self.recorder.starts,
            "scope":"Ruffle PCM playback; getTimer remains wall-clock; stream seeks are not verified"})
    }
}

impl AudioBackend for OfflineAudio {
    fn play(&mut self) {}
    fn pause(&mut self) {}
    ruffle_core::impl_audio_mixer_backend!(recorder);
}
