//! Bounded 2D image particles. Presentation-only state: never stored in Model,
//! saves, preferences or the hidden handoff slot, and never advanced by drawing.
//!
//! Time and randomness live here; story conditions and QuickJS callbacks never see
//! either. A batch of author commands is validated by `interface::host` first and
//! then applied atomically by `System::apply`.
use std::collections::BTreeMap;

/// Declared emitters across a whole UI definition.
pub const MAX_DECLARED: usize = 32;
/// Declared emitters on one screen; the page stack holds at most eight screens, so
/// this also bounds simultaneously active emitters to 32.
pub const MAX_PER_SCREEN: usize = 4;
/// Live particles in one emitter.
pub const MAX_PER_EMITTER: usize = 512;
/// Live particles across all emitters (a hard clamp, never an author error).
pub const MAX_LIVE_TOTAL: usize = 2048;
/// Particles one `burst` command may request.
pub const MAX_BURST: u32 = 128;
/// Particles created in one update across all emitters.
pub const MAX_SPAWN_PER_UPDATE: usize = 256;
/// Longest simulated step; a long frame never spawns or integrates unbounded time.
pub const MAX_STEP: f32 = 0.1;
pub const MIN_LIFE: f32 = 0.1;
pub const MAX_LIFE: f32 = 10.0;
pub const MAX_SPEED: f32 = 2000.0;
pub const MAX_ACCEL: f32 = 2000.0;
pub const MAX_SPIN: f32 = 720.0;
pub const MIN_SIZE: f32 = 0.5;
pub const MAX_SIZE: f32 = 512.0;

/// One explicit author command. Every value arrives from validated UI XML and is
/// re-checked against the visible page stack before it reaches this module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Start,
    Stop,
    Clear,
    Burst,
}

impl Action {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "start" => Self::Start,
            "stop" => Self::Stop,
            "clear" => Self::Clear,
            "burst" => Self::Burst,
            _ => return None,
        })
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Clear => "clear",
            Self::Burst => "burst",
        }
    }
}

/// Static emitter configuration from one `<particles>` node. The owning screen is
/// resolved at load time so a page change can discard exactly its own instances.
#[derive(Clone, Debug)]
pub struct Emitter {
    pub id: String,
    pub screen: String,
    pub image: String,
    pub rect: [f32; 4],
    /// Continuous emission rate in particles per second; zero means burst only.
    pub rate: f32,
    /// Particles per explicit burst; zero means the emitter has no burst.
    pub burst: u32,
    pub max: usize,
    /// Lifetime in seconds: minimum and maximum.
    pub life: [f32; 2],
    /// Particle width in logical UI units: minimum and maximum.
    pub size: [f32; 2],
    /// Initial velocity in logical units per second: `[x range, y range]`.
    pub velocity: [[f32; 2]; 2],
    pub accel: [f32; 2],
    /// Spin in degrees per second: minimum and maximum.
    pub spin: [f32; 2],
    pub color: [u8; 4],
    /// Fade in at birth and out at death; `false` keeps the authored alpha.
    pub fade: bool,
    /// Start automatically while the owning screen is presented.
    pub autostart: bool,
}

/// One live particle in emitter-local logical units.
#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub age: f32,
    pub life: f32,
    pub size: f32,
    pub rotation: f32,
    pub spin: f32,
}

impl Particle {
    /// Lifetime envelope used for the drawn tint. Never mutates the particle.
    pub fn opacity(&self, fade: bool) -> f32 {
        if !fade || self.life <= 0.0 {
            return 1.0;
        }
        let t = (self.age / self.life).clamp(0.0, 1.0);
        (t / 0.2).min(1.0).min(((1.0 - t) / 0.35).clamp(0.0, 1.0))
    }
}

#[derive(Default)]
struct State {
    particles: Vec<Particle>,
    accumulator: f32,
    emitting: bool,
    /// Burst particles waiting for the next update's creation budget.
    pending: u32,
}

/// Deterministic xorshift generator. Host entropy seeds it once; tests seed it
/// explicitly so motion and lifetimes are reproducible.
struct Rng {
    state: u64,
}

impl Rng {
    fn seeded(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u32 << 24) as f32
    }
    fn range(&mut self, min: f32, max: f32) -> f32 {
        if max <= min {
            min
        } else {
            min + (max - min) * self.unit()
        }
    }
}

/// All live particle instances of one UI definition.
pub struct System {
    emitters: BTreeMap<String, Emitter>,
    states: BTreeMap<String, State>,
    /// Emitters explicitly stopped or cleared while their page stays open; the next
    /// visit or an explicit `start` re-arms them.
    suppressed: std::collections::BTreeSet<String>,
    rng: Rng,
}

impl Default for System {
    fn default() -> Self {
        Self::new(BTreeMap::new())
    }
}

impl System {
    pub fn new(emitters: BTreeMap<String, Emitter>) -> Self {
        let mut bytes = [0; 8];
        let seed = if getrandom::fill(&mut bytes).is_ok() {
            u64::from_le_bytes(bytes)
        } else {
            0x5DEE_CE66_D1CE_B00D
        };
        Self::seeded(emitters, seed)
    }
    pub fn seeded(emitters: BTreeMap<String, Emitter>, seed: u64) -> Self {
        Self {
            emitters,
            states: BTreeMap::new(),
            suppressed: std::collections::BTreeSet::new(),
            rng: Rng::seeded(seed),
        }
    }
    pub fn emitters(&self) -> &BTreeMap<String, Emitter> {
        &self.emitters
    }
    pub fn emitter(&self, id: &str) -> Option<&Emitter> {
        self.emitters.get(id)
    }
    pub fn particles(&self, id: &str) -> Option<&[Particle]> {
        self.states
            .get(id)
            .map(|state| state.particles.as_slice())
            .filter(|particles| !particles.is_empty())
    }
    pub fn emitting(&self, id: &str) -> bool {
        self.states.get(id).is_some_and(|state| state.emitting)
    }
    pub fn live(&self) -> usize {
        self.states
            .values()
            .map(|state| state.particles.len() + state.pending as usize)
            .sum()
    }
    /// True while another update can still change the picture.
    pub fn animating(&self) -> bool {
        self.states
            .values()
            .any(|state| state.emitting || !state.particles.is_empty() || state.pending > 0)
    }

    /// Applies one already validated command batch. Unknown IDs are ignored here
    /// because `interface::host` rejects them before any commit.
    pub fn apply(&mut self, commands: &[(String, Action)]) {
        for (id, action) in commands {
            let Some(emitter) = self.emitters.get(id) else {
                continue;
            };
            match action {
                Action::Start => {
                    self.suppressed.remove(id);
                    self.states.entry(id.clone()).or_default().emitting = true;
                }
                Action::Stop => {
                    self.suppressed.insert(id.clone());
                    if let Some(state) = self.states.get_mut(id) {
                        state.emitting = false;
                    }
                }
                Action::Clear => {
                    self.suppressed.insert(id.clone());
                    self.states.remove(id);
                }
                Action::Burst => {
                    let state = self.states.entry(id.clone()).or_default();
                    state.pending = state.pending.saturating_add(emitter.burst).min(MAX_BURST);
                }
            }
        }
    }

    /// Keeps only emitters whose screen is still presented and starts the automatic
    /// ones. Explicit emitters keep their current state while their screen stays open,
    /// and an explicit stop clears only when the page is left.
    pub fn sync(&mut self, stack: &[String]) {
        let visible = |screen: &str| stack.iter().any(|id| id == screen);
        let emitter = &self.emitters;
        self.states
            .retain(|id, _| emitter.get(id).is_some_and(|e| visible(&e.screen)));
        self.suppressed
            .retain(|id| emitter.get(id).is_some_and(|e| visible(&e.screen)));
        let started: Vec<String> = self
            .emitters
            .iter()
            .filter(|(id, emitter)| {
                emitter.autostart && visible(&emitter.screen) && !self.suppressed.contains(*id)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in started {
            self.states.entry(id).or_insert_with(|| State {
                emitting: true,
                ..State::default()
            });
        }
    }

    /// Rebuilds the declared initial state: used by load, restart and hot reload so a
    /// restored picture never replays historical bursts.
    pub fn reset(&mut self, stack: &[String]) {
        self.states.clear();
        self.suppressed.clear();
        self.sync(stack);
    }

    /// Single-command helpers for tests and hosts that already validated the emitter.
    pub fn command(&mut self, id: &str, action: Action) {
        self.apply(&[(id.to_owned(), action)]);
    }
    pub fn start(&mut self, id: &str) {
        self.command(id, Action::Start);
    }
    pub fn stop(&mut self, id: &str) {
        self.command(id, Action::Stop);
    }
    pub fn clear(&mut self, id: &str) {
        self.command(id, Action::Clear);
    }
    pub fn burst(&mut self, id: &str) {
        self.command(id, Action::Burst);
    }

    /// Advances physics and emission. `seconds` is already pause-filtered by the App.
    pub fn tick(&mut self, seconds: f32) {
        if !seconds.is_finite() || seconds <= 0.0 {
            return;
        }
        let dt = seconds.min(MAX_STEP);
        let mut states = std::mem::take(&mut self.states);
        let mut live: usize = states.values().map(|state| state.particles.len()).sum();
        let mut spawned = 0usize;
        for (id, state) in states.iter_mut() {
            let Some(emitter) = self.emitters.get(id).cloned() else {
                state.particles.clear();
                continue;
            };
            for particle in &mut state.particles {
                particle.age += dt;
                particle.vx += emitter.accel[0] * dt;
                particle.vy += emitter.accel[1] * dt;
                particle.x += particle.vx * dt;
                particle.y += particle.vy * dt;
                particle.rotation = (particle.rotation + particle.spin * dt).rem_euclid(360.0);
            }
            let before = state.particles.len();
            state
                .particles
                .retain(|particle| particle.age < particle.life);
            live -= before - state.particles.len();
            if state.emitting && emitter.rate > 0.0 {
                state.accumulator += emitter.rate * dt;
                while state.accumulator >= 1.0 {
                    if state.particles.len() >= emitter.max
                        || live >= MAX_LIVE_TOTAL
                        || spawned >= MAX_SPAWN_PER_UPDATE
                    {
                        break;
                    }
                    state.accumulator -= 1.0;
                    spawn(&mut self.rng, &emitter, &mut state.particles);
                    spawned += 1;
                    live += 1;
                }
                // A capped emitter keeps at most one pending particle so a stalled
                // window cannot release a large catch-up wave later.
                state.accumulator = state.accumulator.min(1.0);
            }
            while state.pending > 0 {
                if state.particles.len() >= emitter.max
                    || live >= MAX_LIVE_TOTAL
                    || spawned >= MAX_SPAWN_PER_UPDATE
                {
                    break;
                }
                state.pending -= 1;
                spawn(&mut self.rng, &emitter, &mut state.particles);
                spawned += 1;
                live += 1;
            }
        }
        self.states = states;
    }
}

fn spawn(rng: &mut Rng, emitter: &Emitter, out: &mut Vec<Particle>) {
    out.push(Particle {
        x: rng.range(0.0, emitter.rect[2]),
        y: rng.range(0.0, emitter.rect[3]),
        vx: rng.range(emitter.velocity[0][0], emitter.velocity[0][1]),
        vy: rng.range(emitter.velocity[1][0], emitter.velocity[1][1]),
        age: 0.0,
        life: rng
            .range(emitter.life[0], emitter.life[1])
            .clamp(MIN_LIFE, MAX_LIFE),
        size: rng
            .range(emitter.size[0], emitter.size[1])
            .clamp(MIN_SIZE, MAX_SIZE),
        rotation: 0.0,
        spin: rng.range(emitter.spin[0], emitter.spin[1]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emitter(id: &str, screen: &str) -> Emitter {
        Emitter {
            id: id.into(),
            screen: screen.into(),
            image: "particle.dot".into(),
            rect: [0.0, 0.0, 100.0, 100.0],
            rate: 10.0,
            burst: 4,
            max: 32,
            life: [0.5, 0.5],
            size: [8.0, 8.0],
            velocity: [[0.0, 0.0], [10.0, 10.0]],
            accel: [0.0, 0.0],
            spin: [0.0, 0.0],
            color: [255; 4],
            fade: true,
            autostart: false,
        }
    }
    fn system(list: Vec<Emitter>) -> System {
        System::seeded(
            list.into_iter().map(|e| (e.id.clone(), e)).collect(),
            0x1234_5678,
        )
    }

    #[test]
    fn automatic_emitters_follow_the_visible_page_and_rebuild_after_reset() {
        let mut auto = emitter("rain", "hud");
        auto.autostart = true;
        let mut system = system(vec![auto, emitter("petals", "menu")]);
        system.sync(&["title".into()]);
        assert!(system.particles("rain").is_none());
        assert!(!system.emitting("rain"));
        system.sync(&["hud".into(), "menu".into()]);
        assert!(system.emitting("rain"));
        system.start("petals");
        system.tick(0.5);
        assert!(system.live() > 0);
        // Closing the page discards its instances and stops its emission.
        system.sync(&["hud".into()]);
        assert!(system.particles("petals").is_none());
        assert!(!system.emitting("petals"));
        system.reset(&["hud".into(), "menu".into()]);
        assert!(system.emitting("rain"));
        assert!(!system.emitting("petals"));
    }

    #[test]
    fn stop_keeps_live_particles_and_clear_removes_them() {
        let mut auto = emitter("rain", "hud");
        auto.autostart = true;
        let mut system = system(vec![auto]);
        system.sync(&["hud".into()]);
        system.tick(0.5);
        let live = system.live();
        assert!(live > 0);
        system.stop("rain");
        assert!(!system.emitting("rain"));
        assert_eq!(system.live(), live);
        system.tick(0.2);
        // No new particles: the survivors only age, never increase.
        assert!(system.live() <= live);
        system.clear("rain");
        assert_eq!(system.live(), 0);
        assert!(!system.emitting("rain"));
    }

    #[test]
    fn explicit_stop_and_clear_suppress_autostart_until_the_page_is_reopened() {
        let mut auto = emitter("rain", "hud");
        auto.autostart = true;
        let mut system = system(vec![auto]);
        let page = ["hud".to_string()];
        system.sync(&page);
        assert!(system.emitting("rain"));
        system.clear("rain");
        assert!(!system.emitting("rain") && system.live() == 0);
        // Ordinary commits while the page stays open must not re-arm the emitter.
        system.sync(&page);
        assert!(!system.emitting("rain"));
        // Leaving and re-entering the page restores the declared auto behaviour.
        system.sync(&[]);
        system.sync(&page);
        assert!(system.emitting("rain"));
        system.stop("rain");
        system.sync(&page);
        assert!(!system.emitting("rain"));
        system.start("rain");
        assert!(system.emitting("rain"));
        system.reset(&page);
        assert!(system.emitting("rain"));
    }

    #[test]
    fn long_frames_and_bursts_stay_within_the_update_budget() {
        let mut auto = emitter("rain", "hud");
        auto.autostart = true;
        auto.rate = 200.0;
        auto.max = MAX_PER_EMITTER;
        let mut system = system(vec![auto]);
        system.sync(&["hud".into()]);
        // One ten second step must not create ten seconds of particles.
        system.tick(10.0);
        assert!(system.live() <= MAX_SPAWN_PER_UPDATE);
        for _ in 0..8 {
            system.tick(0.1);
        }
        assert!(system.live() <= MAX_PER_EMITTER);
        system.apply(&[("rain".into(), Action::Burst)]);
        system.tick(0.1);
        assert!(system.live() <= MAX_PER_EMITTER);
    }

    #[test]
    fn zero_elapsed_time_never_accumulates_or_changes_state() {
        let mut auto = emitter("rain", "hud");
        auto.autostart = true;
        auto.rate = 100.0;
        let mut system = system(vec![auto]);
        system.sync(&["hud".into()]);
        system.tick(0.05);
        let snapshot: Vec<(f32, f32, f32)> = system
            .particles("rain")
            .unwrap()
            .iter()
            .map(|p| (p.x, p.y, p.age))
            .collect();
        system.tick(0.0);
        let after: Vec<(f32, f32, f32)> = system
            .particles("rain")
            .unwrap()
            .iter()
            .map(|p| (p.x, p.y, p.age))
            .collect();
        assert_eq!(snapshot, after);
    }

    #[test]
    fn identical_seeds_produce_identical_motion_and_expiry() {
        let build = || {
            let mut auto = emitter("rain", "hud");
            auto.autostart = true;
            auto.velocity = [[-5.0, 5.0], [20.0, 40.0]];
            auto.spin = [-90.0, 90.0];
            let mut system = system(vec![auto]);
            system.sync(&["hud".into()]);
            for _ in 0..12 {
                system.tick(0.05);
            }
            system
                .particles("rain")
                .unwrap()
                .iter()
                .map(|p| (p.x, p.y, p.rotation, p.age))
                .collect::<Vec<_>>()
        };
        let first = build();
        assert!(!first.is_empty());
        assert_eq!(first, build());
    }

    #[test]
    fn particles_expire_after_their_lifetime_and_stop_animating() {
        let mut auto = emitter("rain", "hud");
        auto.autostart = true;
        auto.rate = 50.0;
        let mut system = system(vec![auto]);
        system.sync(&["hud".into()]);
        system.tick(0.5);
        assert!(system.live() > 0);
        system.stop("rain");
        for _ in 0..30 {
            system.tick(0.1);
        }
        assert_eq!(system.live(), 0);
        assert!(!system.animating());
    }
}
