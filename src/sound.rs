//! Optional synthesized keystroke sound.
//!
//! The `sound` feature enables only rodio's playback backend; no audio decoder
//! formats are linked because the click is generated in memory.

use std::io;

#[cfg(any(feature = "sound", test))]
const CLICK_FREQUENCY_HZ: f32 = 880.0;
#[cfg(any(feature = "sound", test))]
const CLICK_AMPLITUDE: f32 = 0.18;
#[cfg(any(feature = "sound", test))]
const CLICK_DURATION: std::time::Duration = std::time::Duration::from_millis(18);

#[cfg(feature = "sound")]
mod sound_impl {
    use super::*;
    use rodio::source::{SineWave, Source};
    use rodio::{DeviceSinkBuilder, MixerDeviceSink};

    pub struct Ticker {
        sink: Option<MixerDeviceSink>,
    }

    impl Ticker {
        pub fn disabled() -> Self {
            Self { sink: None }
        }

        pub fn new() -> io::Result<Self> {
            DeviceSinkBuilder::open_default_sink()
                .map(|sink| Self { sink: Some(sink) })
                .map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::NotConnected,
                        format!("sound is enabled but audio initialization failed: {error}"),
                    )
                })
        }

        pub fn click(&self) {
            if let Some(sink) = &self.sink {
                // A short audible tone that starts at full time zero, peaks at
                // 18% full scale, and reports the same duration it emits.
                let source = SineWave::new(CLICK_FREQUENCY_HZ)
                    .take_duration(CLICK_DURATION)
                    .amplify(CLICK_AMPLITUDE)
                    .fade_in(std::time::Duration::from_millis(1))
                    .fade_out(std::time::Duration::from_millis(12));
                sink.mixer().add(source);
            }
        }
    }
}

#[cfg(not(feature = "sound"))]
mod sound_impl {
    use super::*;

    pub struct Ticker;

    impl Ticker {
        pub fn disabled() -> Self {
            Self
        }

        pub fn new() -> io::Result<Self> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "sound is enabled in config, but this binary was built without --features sound",
            ))
        }

        pub fn click(&self) {}
    }
}

pub use sound_impl::Ticker;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_parameters_are_audible_and_consistent() {
        assert_eq!(CLICK_DURATION, std::time::Duration::from_millis(18));
        assert!((0.05..=0.5).contains(&CLICK_AMPLITUDE));
        assert!((200.0..=4_000.0).contains(&CLICK_FREQUENCY_HZ));
    }

    #[cfg(not(feature = "sound"))]
    #[test]
    fn requesting_uncompiled_sound_is_an_error() {
        assert!(Ticker::new().is_err());
    }
}
