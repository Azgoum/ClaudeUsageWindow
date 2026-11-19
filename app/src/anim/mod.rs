//! Animation math: easing, tweens, keyframe tracks, ring locks, the reset pill text and the
//! reset, entrance and button timelines.
//!
//! Ported from Phase 1 (git `bcaa1bf`): the `*.ts`/`*.tsx` files these modules name are in
//! that commit's `ui/src/`.

pub mod easing;
pub mod entrance;
pub mod lock;
pub mod pill;
pub mod planned;
pub mod surprise;
pub mod track;
pub mod tween;

pub use entrance::{entrance_end, entrance_fraction, refresh_press, REFRESH_END, REFRESH_SEND_AT};
pub use pill::{pill_frame, PillFrame};
pub use planned::{
    crossfade_frame, planned_frame, RingFrame, CROSSFADE_END, CROSSFADE_PILL_AT, OK_END,
    PLANNED_END, PLANNED_PILL_AT, SURPRISE_END, SURPRISE_PILL_AT,
};
pub use surprise::{glow_alpha, spark_at, sparks, surprise_frame, Spark};
