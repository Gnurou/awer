use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use crate::audio::ClassicMusicPlayer;
use crate::audio::MusicPlayer;
use crate::audio::ProtectedMixer;
use crate::audio::SoundSample;

use anyhow::anyhow;

use super::ClassicMixer;
use super::Mixer;

impl sdl2::audio::AudioCallback for ProtectedMixer<ClassicMixer> {
    type Channel = i8;

    fn callback(&mut self, out: &mut [Self::Channel]) {
        // First set the whole buffer to silence as SDL2 doesn't do it for us.
        for s in out.iter_mut() {
            *s = 0;
        }

        self.0.lock().unwrap().fill_buffer(out)
    }
}

enum MusicTimerState {
    Stopped,
    Running {
        /// SDL2 timer. We need to keep it alive as long as it is running.
        _timer: sdl2::timer::Timer<'static, 'static>,
        /// Interval at which the timer will fire.
        delay: Duration,
        /// Timestamp of the start of the current interval.
        current_interval: Arc<Mutex<Instant>>,
    },
    Paused {
        /// Interval at which the timer will fire.
        delay: Duration,
        /// Time elapsed in the interval before we paused.
        elapsed: Duration,
    },
}

/// Timer that calls a closure every time it expires.
pub struct MusicTimer {
    timer_sys: sdl2::TimerSubsystem,
    state: MusicTimerState,
}

impl MusicTimer {
    fn new(sdl_context: &sdl2::Sdl) -> anyhow::Result<Self> {
        Ok(Self {
            timer_sys: sdl_context.timer().map_err(|s| anyhow!(s))?,
            state: MusicTimerState::Stopped,
        })
    }

    fn set_timer(
        &mut self,
        delay: Duration,
        initial_delay: Duration,
        player: Arc<Mutex<ClassicMusicPlayer>>,
        mixer: Arc<Mutex<ClassicMixer>>,
    ) {
        let current_interval = Arc::new(Mutex::new(Instant::now()));
        let current_interval_cb = Arc::clone(&current_interval);

        // Make sure to stop any currently running timer.
        self.state = MusicTimerState::Stopped;

        let timer = self.timer_sys.add_timer(
            initial_delay.as_millis() as u32,
            Box::new(move || {
                *current_interval_cb.lock().unwrap() = Instant::now();

                let mut player = player.lock().unwrap();
                let mut mixer = mixer.lock().unwrap();
                player.process(&mut *mixer);

                if let ClassicMusicPlayer::Playing { .. } = &*player {
                    delay.as_millis() as u32
                } else {
                    0
                }
            }),
        );

        self.state = MusicTimerState::Running {
            // Safe because we are keeping `timer_sys` alive for as long as `timer` is, and there
            // is no direct reference between the two - only a lifetime requirement.
            // Also the callback steals all the data it uses and has no external reference.
            _timer: unsafe {
                std::mem::transmute::<sdl2::timer::Timer<'_, '_>, sdl2::timer::Timer<'_, '_>>(timer)
            },
            delay,
            current_interval,
        };
    }

    fn pause(&mut self) {
        let old_state = std::mem::replace(&mut self.state, MusicTimerState::Stopped);
        self.state = match old_state {
            MusicTimerState::Running {
                delay,
                current_interval,
                ..
            } => {
                let current_interval = *current_interval.lock().unwrap();

                MusicTimerState::Paused {
                    delay,
                    elapsed: Instant::now().duration_since(current_interval),
                }
            }
            _ => old_state,
        }
    }

    fn resume(&mut self, player: Arc<Mutex<ClassicMusicPlayer>>, mixer: Arc<Mutex<ClassicMixer>>) {
        let old_state = std::mem::replace(&mut self.state, MusicTimerState::Stopped);
        if let MusicTimerState::Paused { delay, elapsed } = old_state {
            self.set_timer(delay, delay.saturating_sub(elapsed), player, mixer);
        }
    }

    fn cancel(&mut self) {
        self.state = MusicTimerState::Stopped;
    }
}

pub struct Sdl2Audio {
    mixer: Arc<Mutex<ClassicMixer>>,
    music_player: Arc<Mutex<ClassicMusicPlayer>>,
    audio_device: sdl2::audio::AudioDevice<ProtectedMixer<ClassicMixer>>,
    timer: MusicTimer,
}

impl Sdl2Audio {
    /// Create a new SDL2 audio device from a SDL context.
    ///
    /// `output_freq` is the desired output frequency of the audio playback. SDL may choose a
    /// different one if it is not supported by the audio system.
    pub fn new(sdl_context: &sdl2::Sdl, output_freq: usize) -> anyhow::Result<Self> {
        let audio = sdl_context.audio().map_err(|s| anyhow!(s))?;

        // Compute buffer size that prevents audio lag. E.g for 22050Hz this will be 256 bytes.
        let samples = (output_freq / 100).checked_next_power_of_two().unwrap();

        let desired_spec = sdl2::audio::AudioSpecDesired {
            freq: Some(output_freq as i32),
            channels: Some(1), // mono
            samples: Some(samples as u16),
        };

        let mut audio_device = audio
            .open_playback(None, &desired_spec, |spec| {
                ProtectedMixer::new(ClassicMixer::new(spec.freq as u32))
            })
            .map_err(|s| anyhow!(s))?;
        audio_device.resume();

        let mixer = Arc::clone(&audio_device.lock().0);

        Ok(Self {
            mixer,
            music_player: Default::default(),
            audio_device,
            timer: MusicTimer::new(sdl_context)?,
        })
    }
}

impl Mixer for Sdl2Audio {
    #[tracing::instrument(level = "trace", skip(self, sample))]
    fn add_sample(&mut self, id: u8, sample: Box<SoundSample>) {
        self.mixer.lock().unwrap().add_sample(id, sample)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn play(&mut self, sample_id: u8, channel: u8, freq: u16, volume: u8) {
        self.mixer
            .lock()
            .unwrap()
            .play(sample_id, channel, freq, volume)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn stop(&mut self, channel: u8) {
        self.mixer.lock().unwrap().stop(channel)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn reset(&mut self) {
        self.mixer.lock().unwrap().reset()
    }
}

impl MusicPlayer for Sdl2Audio {
    fn play_music(&mut self, music: Box<super::MusicModule>, tempo: usize, pos: u16) {
        self.music_player.lock().unwrap().load_module(music, pos);

        self.update_tempo(tempo);
    }

    fn update_tempo(&mut self, tempo: usize) {
        let delay = Duration::from_millis(tempo as u64);

        self.timer.set_timer(
            delay,
            delay,
            Arc::clone(&self.music_player),
            Arc::clone(&self.mixer),
        )
    }

    fn stop_music(&mut self) {
        self.timer.cancel();
        *self.music_player.lock().unwrap() = Default::default();
    }

    fn pause(&mut self) {
        self.timer.pause();
        self.audio_device.pause();
    }

    fn resume(&mut self) {
        self.audio_device.resume();
        self.timer
            .resume(Arc::clone(&self.music_player), Arc::clone(&self.mixer));
    }

    fn take_value_of_0xf4(&self) -> Option<i16> {
        self.music_player.lock().unwrap().take_value_of_0xf4()
    }
}
