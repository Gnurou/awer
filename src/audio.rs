mod music;
pub mod sdl2;

use std::{
    collections::BTreeMap,
    mem::size_of,
    sync::{Arc, Mutex},
};

pub use music::*;

use tracing::{debug, error, warn};

const NUM_AUDIO_CHANNELS: usize = 4;

/// Header of a sound sample.
///
/// Separated from the rest so we can use `std::mem::size_of` and `memoffset::offset_of` on it.
#[repr(C)]
#[derive(Debug)]
struct SoundSampleHeader {
    /// Length of the sample until the loop point (full length is there is no loop point).
    len: u16,
    /// Length of the sample after the loop point (zero if there is no loop point).
    loop_len: u16,
    /// Not sure what that is...
    _fill: u32,
}

impl SoundSampleHeader {
    fn len(&self) -> usize {
        self.len as usize * 2 + self.loop_len as usize * 2
    }
}

/// A loaded resource reinterpreted as a sound sample.
///
/// Samples are single-channel, signed 8-bit and can feature an optional loop point.
#[repr(C)]
#[derive(Debug)]
pub struct SoundSample {
    header: SoundSampleHeader,
    /// Audio sample data.
    data: [i8],
}

impl SoundSample {
    /// Create a new `SoundSample` by reinterpreting a resource's byte data.
    ///
    /// This is highly unsafe and must only be called on resource data which type is
    /// [[crate::res::ResType::Sound]].
    pub unsafe fn from_raw_resource(mut data: Vec<u8>) -> Box<Self> {
        let ptr = data.as_mut_ptr();
        // Remove the size of the header and filler
        let mut data_len = data.len() - size_of::<SoundSampleHeader>();
        std::mem::forget(data);

        let header = ptr as *mut SoundSampleHeader;
        let header = &mut *header;
        // Endianness fixup.
        header.len = u16::from_be(header.len);
        header.loop_len = u16::from_be(header.loop_len);

        // Consistency check.
        if header.len() != data_len {
            warn!(
                "sound resource reported a length of {} bytes, but header says {}",
                header.len(),
                data_len
            );
            data_len = std::cmp::min(header.len(), data_len);
        }

        let slice = core::slice::from_raw_parts(ptr as *const (), data_len);
        let ptr = slice as *const [()] as *const SoundSample;

        Box::from_raw(ptr as *mut SoundSample)
    }

    /// Return the starting position of the loop, if any.
    pub fn loop_pos(&self) -> Option<usize> {
        match self.header.loop_len {
            0 => None,
            _ => Some(self.header.len as usize * 2),
        }
    }

    /// Return the total length of the sample.
    pub fn len(&self) -> usize {
        self.data.len()
    }
}

/// Trait for sound mixers. A mixer is capable of playing audio samples over several channels
/// and mixing them into a single output.
pub trait Mixer {
    /// Load a sample for being played later.
    ///
    /// The `sample` will be referred to by `id` when the `play` method is
    /// invoked to play it.
    fn add_sample(&mut self, id: u8, sample: Box<SoundSample>);

    /// Play an audio effect on a channel.
    ///
    /// sample_id: the ID of the sample to play. The sample has normally been
    /// loaded previously using `add_sample`.
    /// channel: channel to play on. Valid range: [0..3]
    /// freq: frequency of playback, in Hz.
    /// volume: volume of playback, between 0 and 63.
    /// loop_start: whether the sample loops, and if so, at which position of `sample`.
    fn play(&mut self, sample_id: u8, channel: u8, freq: u16, volume: u8);

    /// Stop playback on `channel`.
    fn stop(&mut self, channel: u8);

    /// Stop playback and clear all state, including loaded samples.
    fn reset(&mut self);
}

/// Thread-safe mixer.
struct ProtectedMixer<M: Mixer + Send>(Arc<Mutex<M>>);

impl<M: Mixer + Send> ProtectedMixer<M> {
    fn new(mixer: M) -> Self {
        ProtectedMixer(Arc::new(Mutex::new(mixer)))
    }
}

impl<M: Mixer + Send> Mixer for ProtectedMixer<M> {
    #[tracing::instrument(skip(self, sample))]
    fn add_sample(&mut self, id: u8, sample: Box<SoundSample>) {
        self.0.lock().unwrap().add_sample(id, sample)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn play(&mut self, sample_id: u8, channel: u8, freq: u16, volume: u8) {
        self.0
            .lock()
            .unwrap()
            .play(sample_id, channel, freq, volume)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn stop(&mut self, channel: u8) {
        self.0.lock().unwrap().stop(channel)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn reset(&mut self) {
        self.0.lock().unwrap().reset()
    }
}

pub trait MusicPlayer {
    fn play_music(&mut self, music: Box<MusicModule>, tempo: usize, pos: u16);
    fn update_tempo(&mut self, tempo: usize);
    fn stop_music(&mut self);

    fn pause(&mut self);
    fn resume(&mut self);

    fn take_value_of_0xf4(&self) -> Option<i16>;
}

/// Single channel or a mixer, which can currently be playing something or not.
enum MixerChannel {
    /// Nothing is being played on this channel.
    Inactive,
    /// Something is being played on this channel.
    Active {
        /// ID of the sample currently being played.
        sample_id: u8,
        /// Playback volume.
        volume: u8,
        /// We multiply the current sample position by 256 in order to perform sub-sample
        /// arithmetic. This is the current position times 256, plus an offset between the current
        /// and the next sample.
        chunk_pos: usize,
        /// How much `chunk_pos` should be increased by unit of output. This is a function of the
        /// sample playback rate as well as the audio output rate.
        chunk_inc: usize,
    },
}

impl Default for MixerChannel {
    fn default() -> Self {
        Self::Inactive
    }
}

impl ClassicMixer {
    /// Fill `out` with the next chunk of mixed audio from all our active channels.
    #[tracing::instrument(level = "debug", skip(self, out), fields(size = out.len(), buffer = tracing::field::debug(out.as_ptr())))]
    fn fill_buffer(&mut self, out: &mut [i8]) {
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

                    // Get following sample for interpolation.
                    let next_sample_pos = match sample_pos + 1 {
                        pos if pos >= sample.len() => match loop_pos {
                            None => sample_pos,
                            Some(p) => p,
                        },
                        pos => pos,
                    };

                    // Interpolate.
                    let ilc = (*chunk_pos & 0xff) as isize;
                    let s1 = sample.data[sample_pos] as isize;
                    let s2 = sample.data[next_sample_pos] as isize;
                    let s = (s1 * (0x100 - ilc) + (s2 * ilc)) >> 8;
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

/// Basic 4-channel mixer that mimics the original behavior of the game.
pub struct ClassicMixer {
    /// Channels that can be played onto.
    channels: [MixerChannel; NUM_AUDIO_CHANNELS],
    /// Output frequency at which we will mix.
    output_freq: u32,

    samples: BTreeMap<u8, Box<SoundSample>>,
}

impl ClassicMixer {
    pub fn new(output_freq: u32) -> Self {
        Self {
            channels: Default::default(),
            output_freq,
            samples: Default::default(),
        }
    }
}

impl Mixer for ClassicMixer {
    #[tracing::instrument(level = "trace", skip(self, sample))]
    fn add_sample(&mut self, id: u8, sample: Box<SoundSample>) {
        self.samples.insert(id, sample);
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn play(&mut self, sample_id: u8, channel: u8, freq: u16, volume: u8) {
        let channel = match self.channels.get_mut(channel as usize) {
            None => {
                error!("invalid channel index {}", channel);
                return;
            }
            Some(channel) => channel,
        };

        *channel = MixerChannel::Active {
            sample_id,
            volume,
            chunk_inc: ((freq as usize) << 8) / self.output_freq as usize,
            chunk_pos: 8, // Skip header.
        };
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn stop(&mut self, channel: u8) {
        debug!("channel {}: stop", channel);

        let channel = match self.channels.get_mut(channel as usize) {
            None => {
                error!("invalid channel index {}", channel);
                return;
            }
            Some(channel) => channel,
        };

        *channel = MixerChannel::Inactive;
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn reset(&mut self) {
        self.channels = Default::default();
        self.samples = Default::default();
    }
}

/// Table of desired playback frequencies for the `freq` parameter of the `op_playsound`
/// instruction.
pub const PLAYBACK_FREQUENCY: [u16; 40] = [
    0x0CFF, 0x0DC3, 0x0E91, 0x0F6F, 0x1056, 0x114E, 0x1259, 0x136C, 0x149F, 0x15D9, 0x1726, 0x1888,
    0x19FD, 0x1B86, 0x1D21, 0x1EDE, 0x20AB, 0x229C, 0x24B3, 0x26D7, 0x293F, 0x2BB2, 0x2E4C, 0x3110,
    0x33FB, 0x370D, 0x3A43, 0x3DDF, 0x4157, 0x4538, 0x4998, 0x4DAE, 0x5240, 0x5764, 0x5C9A, 0x61C8,
    0x6793, 0x6E19, 0x7485, 0x7BBD,
];

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use memoffset::offset_of;

    use super::*;

    /// Check that the layout of the [`SoundSample`] structure is as expected.
    #[test]
    fn test_sample_layout() {
        assert_eq!(size_of::<SoundSampleHeader>(), 0x8);
        assert_eq!(offset_of!(SoundSampleHeader, len), 0x0);
        assert_eq!(offset_of!(SoundSampleHeader, loop_len), 0x2);
    }
}
