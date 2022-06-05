mod sdl2;

use log::{debug, error};

const NUM_AUDIO_CHANNELS: usize = 4;

/// Trait for sound mixers. A mixer is capable of playing audio samples over several channels
/// and mixing them into a single output.
pub trait Mixer {
    /// Play an audio effect on a channel.
    ///
    /// sample: the sample to play. Although it is passed as a slice of u8, the data is actually
    /// i8.
    /// channel: channel to play on. Valid range: [0..3]
    /// freq: frequency of playback, in Hz.
    /// volume: volume of playback, between 0 and 63.
    /// loop_start: whether the sample loops, and if so, at which position of `sample`.
    fn play(
        &mut self,
        sample: &[u8],
        channel: u8,
        freq: u16,
        volume: u8,
        loop_start: Option<usize>,
    );

    // TODO add an iterator method that returns mixed samples. On MixerChannel, add an iterator
    // method that returns the next sample value or None if nothing is playing.
}

/// Single channel or a mixer, which can currently be playing something or not.
enum MixerChannel {
    /// Nothing is being played on this channel.
    Inactive,
    /// Something is being played on this channel.
    Active {
        /// Sample currently being played.
        sample: Vec<u8>,
        /// Playback volume.
        volume: u8,
        /// Position of the playback loop, if any.
        loop_start: Option<usize>,
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

/// Basic 4-channel mixer that mimics the original behavior of the game.
pub struct ClassicMixer {
    /// Channels that can be played onto.
    channels: [MixerChannel; NUM_AUDIO_CHANNELS],
    /// Output frequency at which we will mix.
    output_freq: u32,
}

impl ClassicMixer {
    pub fn new(output_freq: u32) -> Self {
        Self {
            channels: Default::default(),
            output_freq,
        }
    }
}

impl Mixer for ClassicMixer {
    fn play(
        &mut self,
        sample: &[u8],
        channel: u8,
        freq: u16,
        volume: u8,
        loop_start: Option<usize>,
    ) {
        debug!(
            "channel {}: play sample length {}, freq {}, volume {}, loop_start {:?}",
            channel,
            sample.len(),
            freq,
            volume,
            loop_start
        );
        let channel = match self.channels.get_mut(channel as usize) {
            None => {
                error!("invalid channel index {}", channel);
                return;
            }
            Some(channel) => channel,
        };

        *channel = MixerChannel::Active {
            sample: sample.to_owned(),
            volume,
            loop_start: loop_start.map(|p| p as usize),
            chunk_inc: ((freq as usize) << 8) / self.output_freq as usize,
            chunk_pos: 8, // Skip header.
        };

        println!(
            "{} {} {}",
            ((freq as usize) << 8) / self.output_freq as usize,
            (freq as usize) << 8,
            self.output_freq
        );
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
