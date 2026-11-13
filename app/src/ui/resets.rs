//! Reset animations' state (port of `Widget.tsx:127-204`): batches of reset events, ring locks,
//! the held views, the pill, the gold glow and the sparks.
//!
//! Everything is a function of the animation clock `t`: a batch starts at the frame that sees
//! a new `reset_seq`, and every later frame samples its timelines at `t - t0`. A ring stays
//! locked to the batch that last took it until that batch's `*_END` (or `LOCK_SAFETY_MS`,
//! whichever first), then hands its values over to the live transitions.

use egui::{Color32, Pos2};
use limits_core::{AppState, LimitKey, ResetKind, RingView};

use super::colors::{GOLD, GREEN};
use super::ring::{arc_stroke, num_fill, ring_number, tick_angle_deg};
use super::rings::Live;
use super::Lang;
use crate::anim::lock::{lock_keys, release, Locks};
use crate::anim::pill::{pill_for, PillSpec};
use crate::anim::{
    crossfade_frame, glow_alpha, pill_frame, planned_frame, spark_at, sparks, surprise_frame,
    PillFrame, RingFrame, Spark, CROSSFADE_END, CROSSFADE_PILL_AT, OK_END, PLANNED_END,
    PLANNED_PILL_AT, SURPRISE_END, SURPRISE_PILL_AT,
};

/// A ring never stays locked longer than this much animation time.
pub const LOCK_SAFETY_MS: f64 = 4500.0;
/// The gold glow's length (`glow_alpha`).
const GLOW_MS: f64 = 4200.0;
/// Sparks fly when the flash peaks, 380 ms in.
const SPARKS_AT_MS: f64 = 380.0;
const SPARK_COUNT: usize = 30;
/// The ring's radius in the 64 px SVG box.
const SPARK_RADIUS: f32 = 23.0;
/// The longest spark flight (`sparks`: 700 + up to 600 ms).
const SPARK_MAX_MS: f64 = 1300.0;

fn idx(key: LimitKey) -> usize {
    match key {
        LimitKey::Session => 0,
        LimitKey::Weekly => 1,
    }
}

/// Which timeline a ring plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Play {
    Planned,
    Surprise,
    /// Reduced motion, either kind.
    Crossfade,
}

impl Play {
    fn of(kind: &ResetKind, reduced: bool) -> Play {
        match (reduced, kind) {
            (true, _) => Play::Crossfade,
            (false, ResetKind::Planned) => Play::Planned,
            (false, ResetKind::Surprise { .. }) => Play::Surprise,
        }
    }

    /// When the ring may show live values again.
    fn end(self) -> f64 {
        match self {
            Play::Planned => PLANNED_END,
            Play::Surprise => SURPRISE_END,
            Play::Crossfade => CROSSFADE_END,
        }
    }

    fn pill_at(self) -> f64 {
        match self {
            Play::Planned => PLANNED_PILL_AT,
            Play::Surprise => SURPRISE_PILL_AT,
            Play::Crossfade => CROSSFADE_PILL_AT,
        }
    }
}

fn ms_since(t0: f64, t: f64) -> f64 {
    ((t - t0) * 1000.0).max(0.0)
}

/// One ring's animation: the batch that started it and the view it holds.
#[derive(Clone, Debug)]
struct RingAnim {
    seq: u64,
    t0: f64,
    play: Play,
    from_used: f64,
    /// The view the ring showed before the batch; drawn until the release.
    held: RingView,
}

impl RingAnim {
    fn frame(&self, t: f64) -> RingFrame {
        let ms = ms_since(self.t0, t);
        let num = ring_number(self.held.used);
        let color = arc_stroke(&self.held);
        let mut f = match self.play {
            Play::Planned => planned_frame(ms, self.from_used, num, color),
            Play::Surprise => surprise_frame(ms, self.from_used, num),
            Play::Crossfade => crossfade_frame(ms, num, color),
        };
        // The number keeps its own fill until the animation recolors it.
        if f.num_color == color {
            f.num_color = num_fill(&self.held);
        }
        f
    }

    /// What the ring shows at its release, for the live transitions to start from: an empty
    /// arc, green after a planned reset, gold after a surprise, the tick hidden where it was.
    fn hand_over(&self) -> Live {
        let (arc_color, num_color) = match self.play {
            Play::Planned => (GREEN, GREEN),
            Play::Surprise => (GOLD, GOLD),
            Play::Crossfade => (arc_stroke(&self.held), num_fill(&self.held)),
        };
        Live {
            arc_fraction: 0.0,
            tick_deg: tick_angle_deg(self.held.expected),
            tick_alpha: 0.0,
            arc_color,
            num_color,
        }
    }
}

#[derive(Clone, Debug)]
struct Batch {
    seq: u64,
    t0: f64,
    pill: Option<PillSpec>,
    /// ms after `t0`: the first pill moment among the batch's rings.
    pill_at: f64,
    pill_decided: bool,
}

#[derive(Clone, Debug)]
struct ShownPill {
    spec: PillSpec,
    t0: f64,
    reduced: bool,
}

#[derive(Clone, Debug)]
struct Burst {
    t0: f64,
    sparks: Vec<Spark>,
}

/// One spark to paint: center (widget-local points), scale of its 5 px dot, alpha, color.
pub type SparkDot = (Pos2, f32, f32, Color32);

#[derive(Clone, Debug, Default)]
pub struct Resets {
    /// The last `reset_seq` seen (`None` before the first frame).
    seen: Option<u64>,
    locks: Locks,
    anims: [Option<RingAnim>; 2],
    batches: Vec<Batch>,
    pill: Option<ShownPill>,
    glow: Option<f64>,
    bursts: Vec<Burst>,
    pill_shows: usize,
}

impl Resets {
    /// The rings went away: nothing stays locked, glowing or flying (the seq is still tracked).
    pub fn clear(&mut self, seq: u64) {
        *self = Resets {
            seen: Some(seq),
            pill_shows: self.pill_shows,
            ..Resets::default()
        };
    }

    /// Starts a batch when `state.reset_seq` is new and the rings show. `shown` are the views
    /// drawn on the previous frame (the held ones), `centers` the rings' centers in widget-local
    /// points (sparks start there).
    #[expect(clippy::too_many_arguments)]
    pub fn observe(
        &mut self,
        state: &AppState,
        shown: Option<&[RingView; 2]>,
        live: &[RingView; 2],
        centers: [Pos2; 2],
        t: f64,
        reduced: bool,
        lang: Lang,
    ) {
        let seq = state.reset_seq;
        if self.seen.replace(seq).is_none_or(|s| s == seq) || state.resets.is_empty() {
            return;
        }
        let mut pill_at = f64::INFINITY;
        let mut surprise = false;
        for ev in &state.resets {
            let i = idx(ev.key);
            let play = Play::of(&ev.kind, reduced);
            pill_at = pill_at.min(play.pill_at());
            surprise |= play == Play::Surprise;
            // A ring still locked keeps the view it held; otherwise what was on screen.
            let held = match &self.anims[i] {
                Some(a) if self.locks[i] == Some(a.seq) => a.held.clone(),
                _ => shown
                    .and_then(|s| s.iter().find(|v| v.key == ev.key))
                    .or_else(|| live.iter().find(|v| v.key == ev.key))
                    .cloned()
                    .unwrap_or_else(|| live[i].clone()),
            };
            if play == Play::Surprise {
                let fraction = (ev.from_used / 100.0).max(0.15) as f32;
                let seed = seq.wrapping_mul(2).wrapping_add(i as u64);
                self.bursts.push(Burst {
                    t0: t + SPARKS_AT_MS / 1000.0,
                    sparks: sparks(seed, centers[i], SPARK_RADIUS, fraction, SPARK_COUNT),
                });
            }
            self.anims[i] = Some(RingAnim {
                seq,
                t0: t,
                play,
                from_used: ev.from_used,
                held,
            });
        }
        let keys: Vec<LimitKey> = state.resets.iter().map(|e| e.key).collect();
        self.locks = lock_keys(self.locks, &keys, seq);
        if surprise {
            self.glow = Some(t);
        }
        self.batches.push(Batch {
            seq,
            t0: t,
            pill: pill_for(&state.resets, lang),
            pill_at,
            pill_decided: false,
        });
    }

    fn owns_a_ring(&self, seq: u64) -> bool {
        self.locks.contains(&Some(seq))
    }

    /// Shows batch `b`'s pill once, only while it still owns a ring.
    fn show_pill(&mut self, b: usize, reduced: bool) {
        let batch = &self.batches[b];
        if batch.pill_decided {
            return;
        }
        let (seq, at, spec) = (
            batch.seq,
            batch.t0 + batch.pill_at / 1000.0,
            batch.pill.clone(),
        );
        self.batches[b].pill_decided = true;
        if let (true, Some(spec)) = (self.owns_a_ring(seq), spec) {
            self.pill_shows += 1;
            self.pill = Some(ShownPill {
                spec,
                t0: at,
                reduced,
            });
        }
    }

    /// Moves every timeline to `t`: shows due pills, releases finished rings and drops what is
    /// over. Returns the rings released this frame with the values they hand over.
    pub fn advance(&mut self, t: f64, reduced: bool) -> Vec<(usize, Live)> {
        for b in 0..self.batches.len() {
            let batch = &self.batches[b];
            if ms_since(batch.t0, t) >= batch.pill_at {
                self.show_pill(b, reduced);
            }
        }
        let mut released = vec![];
        for i in 0..2 {
            let Some(anim) = self.anims[i].clone() else {
                continue;
            };
            if self.locks[i] != Some(anim.seq) {
                continue;
            }
            if ms_since(anim.t0, t) >= anim.play.end().min(LOCK_SAFETY_MS) {
                // A stalled animation never reached its pill moment: it still gets its pill.
                if let Some(b) = self.batches.iter().position(|b| b.seq == anim.seq) {
                    self.show_pill(b, reduced);
                }
                self.locks = release(self.locks, key_of(i), anim.seq);
                released.push((i, anim.hand_over()));
            }
        }
        // Drop what is over.
        for i in 0..2 {
            let keep = self.anims[i].as_ref().is_some_and(|a| {
                self.locks[i] == Some(a.seq)
                    || (a.play == Play::Planned && ms_since(a.t0, t) < OK_END)
            });
            if !keep {
                self.anims[i] = None;
            }
        }
        let locks = self.locks;
        self.batches.retain(|b| locks.contains(&Some(b.seq)));
        if self.pill.as_ref().is_some_and(|p| {
            pill_frame(ms_since(p.t0, t), p.spec.hold_ms as f64, p.reduced).is_none()
        }) {
            self.pill = None;
        }
        if self.glow.is_some_and(|g| ms_since(g, t) >= GLOW_MS) {
            self.glow = None;
        }
        self.bursts.retain(|b| ms_since(b.t0, t) < SPARK_MAX_MS);
        released
    }

    /// The view ring `i` holds while locked.
    pub fn held(&self, i: usize) -> Option<&RingView> {
        let a = self.anims[i].as_ref()?;
        (self.locks[i] == Some(a.seq)).then_some(&a.held)
    }

    /// Ring `i`'s animation frame while it is locked.
    pub fn frame(&self, i: usize, t: f64) -> Option<RingFrame> {
        self.held(i)?;
        self.anims[i].as_ref().map(|a| a.frame(t))
    }

    /// The ✓ after ring `i`'s name (alpha, scale), on its own timeline past the release.
    pub fn ok(&self, i: usize, t: f64) -> Option<(f32, f32)> {
        let a = self.anims[i].as_ref()?;
        if a.play != Play::Planned {
            return None;
        }
        let f = a.frame(t);
        (f.ok_alpha > 0.0).then_some((f.ok_alpha, f.ok_scale))
    }

    pub fn locks(&self) -> Locks {
        self.locks
    }

    /// How many pills were shown (one per batch at most).
    pub fn pill_shows(&self) -> usize {
        self.pill_shows
    }

    /// The pill and its frame at `t`, once its moment came.
    pub fn pill(&self, t: f64) -> Option<(&PillSpec, PillFrame)> {
        let p = self.pill.as_ref()?;
        if t < p.t0 {
            return None;
        }
        let f = pill_frame(ms_since(p.t0, t), p.spec.hold_ms as f64, p.reduced)?;
        Some((&p.spec, f))
    }

    /// The gold glow's alpha at `t` (0 without a surprise).
    pub fn glow_alpha(&self, t: f64) -> f32 {
        self.glow.map_or(0.0, |g| glow_alpha(ms_since(g, t)))
    }

    /// The sparks in flight at `t`.
    pub fn sparks(&self, t: f64) -> Vec<SparkDot> {
        let mut dots = vec![];
        for b in self.bursts.iter().filter(|b| t >= b.t0) {
            let ms = ms_since(b.t0, t);
            for s in &b.sparks {
                if let Some((at, scale, alpha)) = spark_at(s, ms) {
                    dots.push((at, scale, alpha, s.color));
                }
            }
        }
        dots
    }

    /// Something is still playing (as of the last `advance`).
    pub fn animating(&self) -> bool {
        self.locks.iter().any(Option::is_some)
            || self.anims.iter().any(Option::is_some)
            || self.pill.is_some()
            || self.glow.is_some()
            || !self.bursts.is_empty()
    }
}

fn key_of(i: usize) -> LimitKey {
    if i == 0 {
        LimitKey::Session
    } else {
        LimitKey::Weekly
    }
}
