pub struct SyncState {
    // First observations — used as baseline
    first_video_rtp_ts: Option<u32>,
    first_audio_samples: Option<u64>,
    // Latest observations
    last_video_rtp_ts: u32,
    audio_samples_played: u64,
    // Derived
    rate_adjustment: f64,
}

impl Default for SyncState {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncState {
    pub fn new() -> Self {
        Self {
            first_video_rtp_ts: None,
            first_audio_samples: None,
            last_video_rtp_ts: 0,
            audio_samples_played: 0,
            rate_adjustment: 1.0,
        }
    }

    pub fn rate_adjustment(&self) -> f64 {
        self.rate_adjustment
    }

    pub fn report_audio_played(&mut self, samples: u64) {
        if self.first_audio_samples.is_none() && samples > 0 {
            self.first_audio_samples = Some(samples);
        }
        self.audio_samples_played = samples;
    }

    pub fn report_video_rendered(&mut self, rtp_ts: u32) {
        if self.first_video_rtp_ts.is_none() {
            self.first_video_rtp_ts = Some(rtp_ts);
        }
        self.last_video_rtp_ts = rtp_ts;
        self.update_drift();
    }

    fn update_drift(&mut self) {
        let (Some(base_video), Some(base_audio)) =
            (self.first_video_rtp_ts, self.first_audio_samples)
        else {
            return;
        };

        // Relative progress since first observation
        let video_elapsed_ts = self.last_video_rtp_ts.wrapping_sub(base_video) as u64;
        let audio_elapsed_samples = self.audio_samples_played.saturating_sub(base_audio);

        // Convert to milliseconds
        let video_ms = video_elapsed_ts * 1000 / 90000;
        let audio_ms = audio_elapsed_samples * 1000 / 48000;

        // Need at least 500ms of data before judging drift
        if video_ms < 500 || audio_ms < 500 {
            return;
        }

        // Negative drift means audio ahead (expected due to jitter buffer prefill).
        // Threshold accounts for the 40ms jitter buffer structural offset.
        let drift_ms = video_ms as i64 - audio_ms as i64;

        if drift_ms > 50 {
            self.rate_adjustment = 1.02; // video ahead: speed up audio
        } else if drift_ms < -50 {
            self.rate_adjustment = 0.98; // audio ahead: slow down audio
        } else if drift_ms.unsigned_abs() < 20 {
            self.rate_adjustment = 1.0;
        }
    }
}
