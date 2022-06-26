use crate::audio::SoundSample;

use anyhow::anyhow;
use log::{debug, warn};

use super::{ClassicMixer, Mixer, MixerChannel};

impl sdl2::audio::AudioCallback for ClassicMixer {
    type Channel = i8;

    fn callback(&mut self, out: &mut [Self::Channel]) {
        // First set the whole buffer to silence as SDL2 doesn't do it for us.
        for s in out.iter_mut() {
            *s = 0;
        }

        for (ch_id, channel) in &mut self.channels.iter_mut().enumerate() {
            if let MixerChannel::Active {
                sample_id,
                volume,
                chunk_pos,
                chunk_inc,
            } = channel
            {
                let sample = match self.samples.get(sample_id) {
                    Some(sample) => sample,
                    None => {
                        warn!("sample {:02x} is not loaded, aborting playback", sample_id);
                        *channel = MixerChannel::Inactive;
                        continue;
                    }
                };
                let loop_pos = sample.loop_pos();

                'chan: for c in out.iter_mut() {
                    let mut sample_pos = *chunk_pos >> 8;
                    let delta = *chunk_pos & 0xff;

                    if sample_pos >= sample.len() {
                        match loop_pos {
                            None => {
                                debug!("channel {}: stop as end of sample reached", ch_id);
                                *channel = MixerChannel::Inactive;
                                break 'chan;
                            }
                            Some(p) => {
                                debug!("channel {}: looping", ch_id,);
                                sample_pos = p + sample_pos - sample.len();
                                *chunk_pos = (sample_pos << 8) + delta;
                            }
                        }
                    }

                    // The sample is not stored as u8 but i8 in the resource.
                    let s = sample.data[sample_pos] as i8;
                    // Apply volume.
                    let v = s as i16 * *volume as i16 / 0x40;
                    // Mix and clamp.
                    let b = v + *c as i16;
                    *c = match b {
                        v if v < i8::MIN as i16 => i8::MIN,
                        v if v > i8::MAX as i16 => i8::MAX,
                        _ => b as i8,
                    };

                    *chunk_pos += *chunk_inc;
                }
            }
        }
    }
}

impl Mixer for sdl2::audio::AudioDevice<ClassicMixer> {
    fn add_sample(&mut self, id: usize, sample: Box<SoundSample>) {
        self.lock().add_sample(id, sample)
    }

    fn play(&mut self, sample_id: usize, channel: u8, freq: u16, volume: u8) {
        self.lock().play(sample_id, channel, freq, volume)
    }

    fn reset(&mut self) {
        self.lock().reset()
    }
}

pub struct Sdl2Audio {
    mixer: sdl2::audio::AudioDevice<ClassicMixer>,
}

impl Sdl2Audio {
    /// Create a new SDL2 audio device from a SDL context.
    ///
    /// `output_freq` is the desired output frequency of the audio playback. SDL may choose a
    /// different one if it is not supported by the audio system.
    pub fn new(sdl_context: &sdl2::Sdl, output_freq: usize) -> anyhow::Result<Self> {
        let audio = sdl_context.audio().map_err(|s| anyhow!(s))?;

        let desired_spec = sdl2::audio::AudioSpecDesired {
            freq: Some(output_freq as i32),
            channels: Some(1), // mono
            samples: None,     // default sample size
        };

        let audio_device = audio
            .open_playback(None, &desired_spec, |spec| {
                crate::audio::ClassicMixer::new(spec.freq as u32)
            })
            .map_err(|s| anyhow!(s))?;
        audio_device.resume();

        Ok(Self {
            mixer: audio_device,
        })
    }
}

impl Mixer for Sdl2Audio {
    fn add_sample(&mut self, id: usize, sample: Box<SoundSample>) {
        self.mixer.add_sample(id, sample)
    }

    fn play(&mut self, sample_id: usize, channel: u8, freq: u16, volume: u8) {
        self.mixer.play(sample_id, channel, freq, volume)
    }

    fn reset(&mut self) {
        self.mixer.reset()
    }
}
