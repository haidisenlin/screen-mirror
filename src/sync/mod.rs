pub struct SyncState {
    audio_samples_played: u64,
    last_video_rtp_ts: u32,
    drift_samples: i64,
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
            audio_samples_played: 0,
            last_video_rtp_ts: 0,
            drift_samples: 0,
            rate_adjustment: 1.0,
        }
    }

    pub fn rate_adjustment(&self) -> f64 {
        self.rate_adjustment
    }

    pub fn report_audio_played(&mut self, samples: u64) {
        self.audio_samples_played = samples;
    }

    pub fn report_video_rendered(&mut self, rtp_ts: u32) {
        self.last_video_rtp_ts = rtp_ts;
        self.update_drift();
    }

    fn update_drift(&mut self) {
        let audio_time_ms = (self.audio_samples_played * 1000) / 48000;
        let video_time_ms = (self.last_video_rtp_ts as u64 * 1000) / 90000;

        if audio_time_ms == 0 || video_time_ms == 0 {
            return;
        }

        let drift_ms = video_time_ms as i64 - audio_time_ms as i64;
        self.drift_samples = drift_ms * 48;

        if drift_ms > 30 {
            self.rate_adjustment = 1.02;
        } else if drift_ms < -30 {
            self.rate_adjustment = 0.98;
        } else if drift_ms.unsigned_abs() < 10 {
            self.rate_adjustment = 1.0;
        }
    }
}
