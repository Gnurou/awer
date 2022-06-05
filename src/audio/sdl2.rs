use super::{ClassicMixer, Mixer, MixerChannel};
use log::debug;

impl sdl2::audio::AudioCallback for ClassicMixer {
    type Channel = i8;

    fn callback(&mut self, out: &mut [Self::Channel]) {
        // First set the whole buffer to silence as SDL2 doesn't do it for us.
        for s in out.iter_mut() {
            *s = 0;
        }

        for (ch_id, channel) in &mut self.channels.iter_mut().enumerate() {
            if let MixerChannel::Active {
                sample,
                volume,
                chunk_pos,
                chunk_inc,
                loop_start,
            } = channel
            {
                'chan: for c in out.iter_mut() {
                    let mut sample_pos = *chunk_pos >> 8;
                    let delta = *chunk_pos & 0xff;

                    if sample_pos >= sample.len() {
                        match *loop_start {
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
                    let s = sample[sample_pos] as i8;
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
    fn play(
        &mut self,
        sample: &[u8],
        channel: u8,
        freq: u16,
        volume: u8,
        loop_start: Option<usize>,
    ) {
        self.lock().play(sample, channel, freq, volume, loop_start)
    }
}
