pub fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

pub fn window(p: f32, start: f32, end: f32) -> f32 {
    smoothstep((p - start) / (end - start))
}

pub fn approach_k(dt: f32, tau: f32) -> f32 {
    1.0 - (-dt.min(0.05) / tau.max(1e-6)).exp()
}

pub fn approach(cur: f32, target: f32, dt: f32, tau: f32) -> f32 {
    cur + (target - cur) * approach_k(dt, tau)
}

#[derive(Debug, Clone, Copy)]
pub struct Tween {
    pub x: f32,
    pub target: f32,
    rate: f32,
}

impl Tween {
    pub fn for_duration_ms(x: f32, ms: f32) -> Self {
        Self {
            x,
            target: x,
            rate: 1000.0 / ms.max(1.0),
        }
    }

    pub fn set_duration_ms(&mut self, ms: f32) {
        self.rate = 1000.0 / ms.max(1.0);
    }

    #[cfg(test)]
    pub fn duration_ms(&self) -> f32 {
        1000.0 / self.rate
    }

    pub fn retarget(&mut self, target: f32) {
        self.target = target;
    }

    pub fn snap(&mut self, value: f32) {
        self.x = value;
        self.target = value;
    }

    pub fn run(&mut self, from: f32, to: f32) {
        self.x = from;
        self.target = to;
    }

    pub fn tick(&mut self, dt: f32) -> bool {
        let step = self.rate * dt.min(0.05);
        if self.x < self.target {
            self.x = (self.x + step).min(self.target);
        } else if self.x > self.target {
            self.x = (self.x - step).max(self.target);
        }
        if (self.x - self.target).abs() < 1e-4 {
            self.x = self.target;
        }
        !self.settled()
    }

    pub fn settled(&self) -> bool {
        self.x == self.target
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionTier {
    Fast,
    Standard,
    Slow,
}
#[derive(Debug, Clone, Copy)]
pub struct MotionProfile {
    fast: f32,
    standard: f32,
    slow: f32,
}
impl Default for MotionProfile {
    fn default() -> Self {
        Self::new(180.0, 250.0, 450.0)
    }
}
impl MotionProfile {
    pub fn new(fast: f32, standard: f32, slow: f32) -> Self {
        Self {
            fast: fast.clamp(35.0, 2000.0),
            standard: standard.clamp(35.0, 2000.0),
            slow: slow.clamp(35.0, 4000.0),
        }
    }
    pub fn duration_ms(self, tier: MotionTier) -> f32 {
        match tier {
            MotionTier::Fast => self.fast,
            MotionTier::Standard => self.standard,
            MotionTier::Slow => self.slow,
        }
    }
    pub fn approach(self, current: f32, target: f32, dt: f32, tier: MotionTier) -> f32 {
        approach(current, target, dt, self.duration_ms(tier) / 4000.0)
    }
    pub fn tween(self, x: f32, tier: MotionTier) -> Tween {
        Tween::for_duration_ms(x, self.duration_ms(tier))
    }
}
