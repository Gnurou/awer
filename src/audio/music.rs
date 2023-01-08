//! Support for music playback.
//!
//! Music is represented as a rudimentary 4-channel module format, where samples are regular sound
//! resources and the music sheet is organized as a list of patterns, which playback order is
//! decided by another list. As the current pattern unrolls and notes are encountered, the
//! corresponding sample is sent to the [`Mixer`](crate::audio::Mixer) to be played at the frequency
//! corresponding to the note.

use std::fmt::Debug;
use std::mem::size_of;
use std::num::NonZeroU16;

use tracing::debug;

use crate::audio::Mixer;

/// Information about instruments in a music module.
#[repr(C)]
#[derive(Debug)]
pub struct InstrumentInfo {
    pub res_num: u16,
    pub volume: u16,
}

const NUM_INSTRUMENTS: usize = 15;
const ORDER_TABLE_LEN: u16 = 0x80;

/// Header of a music module.
///
/// Separated from the rest so we can use `std::mem::size_of` and `memoffset::offset_of` on it.
#[repr(C)]
#[derive(Debug)]
pub struct MusicModuleHeader {
    pub delay: u16,
    pub instruments: [InstrumentInfo; NUM_INSTRUMENTS],
    pub num_order: u16,
    pub order_table: [u8; ORDER_TABLE_LEN as usize],
}

/// A note is represented by two words, the first one being the note itself (with a few special
/// values), and the second describing the sample to use, and an optional effect to apply.
///
/// First the special values of the note:
///
/// * `0xFFFE` stops playback on the channel.
/// * `0xFFFD` sets the value contained in the second word into VM register `0xF4`. This one is
///   really annoying as it creates a link between the music player (which runs in an interrupt
///   handler or a different thread) and the VM registers.
/// * `0x0000` does not play any note but apply effects on an already-playing note.
/// * Other values between [0x37..0x1000[ are valid notes. The fina playback frequency is
///   (7159092 / (note * 2)).
///
/// If the first word contains a note, the second word's layout is as follows:
/// * `0xF000` is the instrument number to play (starting at index 1, zero skipping the note
///   completely).
/// * `0x0F00` is the effect. There are only two: `0x5` is volume up, and `0x6` volume down.
/// * `0x00FF` is the parameter of the effect, i.e. the amount by which the volume should go up or
///   down. Final volume must remain in the range [0x0..0x3F].
#[repr(C)]
#[derive(Debug)]
pub struct PatternNote(u16, u16);

#[derive(Debug)]
enum SampleEffect {
    VolumeUp(u8),
    VolumeDown(u8),
}

#[derive(Debug)]
enum NoteType {
    Stop,
    Set0xF4(i16),
    Play {
        // TODO This should be a NonZeroU8?
        sample: u8,
        freq: NonZeroU16,
        effect: Option<SampleEffect>,
    },
}

impl PatternNote {
    fn parse(&self) -> Option<NoteType> {
        match self.0 {
            0xfffe => Some(NoteType::Stop),
            0xfffd => Some(NoteType::Set0xF4(self.1 as i16)),
            note @ 0x37..=0xfff => {
                let sample = ((self.1 & 0xF000) >> 12) as u8;
                let freq = NonZeroU16::new((7159092u32 / (note as u32 * 2)) as u16).unwrap();
                let param = (self.1 & 0x00FF) as u8;
                let effect = match ((self.1 & 0x0F00) >> 8) as u8 {
                    5 => Some(SampleEffect::VolumeUp(param)),
                    6 => Some(SampleEffect::VolumeDown(param)),
                    _ => None,
                };

                Some(NoteType::Play {
                    sample,
                    freq,
                    effect,
                })
            }
            _ => None,
        }
    }
}

pub type PatternLine = [PatternNote; 4];

const LINES_PER_PATTERN: u8 = 64;

#[repr(C)]
#[derive(Debug)]
pub struct MusicPattern {
    pub lines: [PatternLine; LINES_PER_PATTERN as usize],
}

/// A piece of music to be played during the game.
#[repr(C)]
#[derive(Debug)]
pub struct MusicModule {
    pub header: MusicModuleHeader,
    pub patterns: [MusicPattern],
}

impl MusicModule {
    /**
     * Converts the passed raw resource into a `MusicModule`.
     *
     * The caller must guarantee that the passed array of bytes comes from a resource of type
     * [`crate::res::ResType::Music`].
     */
    pub unsafe fn from_raw_resource(mut data: Vec<u8>) -> Box<Self> {
        let ptr = data.as_mut_ptr();
        // Remove the size of the header.
        let patterns_len = data.len() - size_of::<MusicModuleHeader>();
        std::mem::forget(data);

        // The remainder of the data must cover a number of full patterns.
        let num_patterns = patterns_len / size_of::<MusicPattern>();
        assert_eq!(num_patterns * size_of::<MusicPattern>(), patterns_len);

        let slice = core::slice::from_raw_parts(ptr as *const (), num_patterns);
        let ptr = slice as *const [()] as *const MusicModule;
        let mut music = Box::from_raw(ptr as *mut MusicModule);

        // Endianness fixup.
        music.header.delay = u16::from_be(music.header.delay);
        for instrument in &mut music.header.instruments {
            instrument.res_num = u16::from_be(instrument.res_num);
            instrument.volume = u16::from_be(instrument.volume);
        }
        music.header.num_order = u16::from_be(music.header.num_order);
        music
            .patterns
            .iter_mut()
            .flat_map(|pattern| pattern.lines.iter_mut())
            .flat_map(|line| line.iter_mut())
            .for_each(|note| {
                note.0 = u16::from_be(note.0);
                note.1 = u16::from_be(note.1);
            });

        music
    }
}

/// A music player for music modules found in the original game.
pub enum ClassicMusicPlayer {
    Stopped,
    Playing {
        music: Box<MusicModule>,
        // Index in the order table of the current pattern.
        current_order: u16,
        // Line to play in the current pattern.
        current_line: u8,
        // Value of the 0xf4 register, to be set to the VM before the next cycle.
        value_of_0xf4: Option<i16>,
    },
}

impl Debug for ClassicMusicPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stopped => write!(f, "Stopped"),
            Self::Playing {
                current_order,
                current_line,
                value_of_0xf4,
                ..
            } => f
                .debug_struct("Playing")
                .field("current_order", current_order)
                .field("current_line", current_line)
                .field("value_of_0xf4", value_of_0xf4)
                .finish(),
        }
    }
}

impl Default for ClassicMusicPlayer {
    fn default() -> Self {
        ClassicMusicPlayer::Stopped
    }
}

impl ClassicMusicPlayer {
    /// Load `music` as the current volume, with playback starting from pattern `pos`.
    pub fn load_module(&mut self, music: Box<MusicModule>, pos: u16) {
        *self = ClassicMusicPlayer::Playing {
            music,
            current_order: pos,
            current_line: 0,
            value_of_0xf4: None,
        };
    }

    /// Process the next line in the pattern, doing playback on `mixer`.
    #[tracing::instrument(level = "trace", skip(mixer), fields(value_of_0xf4))]
    pub fn process<M: Mixer>(&mut self, mixer: &mut M) {
        match self {
            ClassicMusicPlayer::Stopped => (),
            ClassicMusicPlayer::Playing {
                music,
                current_order,
                current_line,
                value_of_0xf4,
            } => {
                let current_pattern = music.header.order_table[*current_order as usize];
                let pattern = &music.patterns[current_pattern as usize];
                let line = &pattern.lines[*current_line as usize];

                for (chan, note) in line
                    .iter()
                    .enumerate()
                    .filter_map(|(i, note)| note.parse().map(|note| (i as u8, note)))
                {
                    match note {
                        NoteType::Stop => mixer.stop(chan),
                        NoteType::Set0xF4(value) => {
                            *value_of_0xf4 = Some(value);
                            debug!("set 0xf4 to {:04x}", value);
                        }
                        NoteType::Play {
                            sample,
                            freq,
                            effect,
                        } => {
                            let instrument = &music.header.instruments[sample as usize - 1];
                            let sample = instrument.res_num as u8;
                            let mut volume = instrument.volume as i16;

                            match effect {
                                None => (),
                                Some(SampleEffect::VolumeUp(param)) => volume += param as i16,
                                Some(SampleEffect::VolumeDown(param)) => volume -= param as i16,
                            }

                            // Clamp into valid volume range.
                            volume = std::cmp::min(volume, 0x3F);
                            volume = std::cmp::max(volume, 0x0);

                            mixer.play(sample, chan, freq.into(), volume as u8);
                        }
                    }
                }

                tracing::Span::current().record("value_of_0xf4", value_of_0xf4);

                *current_line += 1;
                if *current_line >= LINES_PER_PATTERN {
                    *current_line = 0;
                    *current_order += 1;
                    if *current_order >= music.header.num_order {
                        *self = ClassicMusicPlayer::Stopped;
                    }
                }
            }
        }
    }

    pub fn take_value_of_0xf4(&mut self) -> Option<i16> {
        match self {
            ClassicMusicPlayer::Playing { value_of_0xf4, .. } => value_of_0xf4.take(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use memoffset::offset_of;

    use super::*;

    /// Check that the layout of the [`InstrumentInfo`] structure is as expected.
    #[test]
    fn test_instrument_info_layout() {
        assert_eq!(offset_of!(InstrumentInfo, res_num), 0x0);
        assert_eq!(offset_of!(InstrumentInfo, volume), 0x2);
    }

    /// Check that the layout of the [`MusicPattern`] structure is as expected.
    #[test]
    fn test_pattern_layout() {
        assert_eq!(size_of::<MusicPattern>(), 0x400);
    }

    /// Check that the layout of the [`MusicModule`] structure is as expected.
    #[test]
    fn test_module_layout() {
        assert_eq!(size_of::<MusicModuleHeader>(), 0xc0);
        assert_eq!(offset_of!(MusicModuleHeader, delay), 0x0);
        assert_eq!(offset_of!(MusicModuleHeader, instruments), 0x2);
        assert_eq!(offset_of!(MusicModuleHeader, num_order), 0x3e);
        assert_eq!(offset_of!(MusicModuleHeader, order_table), 0x40);
    }
}
