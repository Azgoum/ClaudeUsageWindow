import { animate, type AnimationPlaybackControls, type DOMKeyframesDefinition, type Easing } from 'motion'

/** Prototype `A()` default easing: cubic-bezier(.2,.8,.2,1). */
export const EASE_DEFAULT: Easing = [0.2, 0.8, 0.2, 1]

export interface PlayOpts {
  /** ms, like the prototype */
  duration: number
  ease?: Easing | ((t: number) => number)
  times?: number[]
  delay?: number
}

export const prefersReducedMotion = (): boolean =>
  typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches

/** motion's `animate` with the prototype's units (ms) and its default easing. */
export function run(el: Element, kf: DOMKeyframesDefinition, o: PlayOpts): AnimationPlaybackControls {
  return animate(el, kf, {
    duration: o.duration / 1000,
    delay: (o.delay ?? 0) / 1000,
    ease: o.ease ?? EASE_DEFAULT,
    times: o.times,
  })
}

// Inline styles motion leaves behind (it commits the final keyframe to `style`, even
// stroke-dashoffset on SVG, which would then override the attribute React renders).
const STYLE_PROPS: Record<string, string[]> = {
  strokeDashoffset: ['stroke-dashoffset'],
  opacity: ['opacity'],
  filter: ['filter'],
  transform: ['transform', 'transform-origin', 'transform-box'],
}

/**
 * One ring's running reset animation. Everything it starts can be stopped at once, its waits
 * resolve early when stopped, and `clear` wipes the inline styles it left (prototype `unlock`).
 */
export class Stage {
  aborted = false
  private controls: AnimationPlaybackControls[] = []
  private touched = new Map<HTMLElement | SVGElement, Set<string>>()
  private wakers = new Set<() => void>()
  private undo: (() => void)[] = []

  /** Starts an animation; resolves when it ends or the stage is stopped (never hangs on abort). */
  play(el: HTMLElement | SVGElement | null, kf: DOMKeyframesDefinition, o: PlayOpts): Promise<void> {
    if (!el || this.aborted) return Promise.resolve()
    const props = this.touched.get(el) ?? new Set<string>()
    for (const k of Object.keys(kf)) for (const p of STYLE_PROPS[k] ?? []) props.add(p)
    this.touched.set(el, props)
    const c = run(el, kf, o)
    this.controls.push(c)
    return this.race(c.finished.then(() => undefined, () => undefined))
  }

  wait(ms: number): Promise<void> {
    return this.race(new Promise((r) => setTimeout(r, ms)))
  }

  /** Runs `fn` when the stage is cleared (restores something the animation changed by hand). */
  onClear(fn: () => void) {
    this.undo.push(fn)
  }

  stop() {
    this.aborted = true
    for (const c of this.controls) c.cancel()
    this.controls = []
    for (const w of this.wakers) w()
    this.wakers.clear()
  }

  clear() {
    for (const [el, props] of this.touched) for (const p of props) el.style.removeProperty(p)
    this.touched.clear()
    for (const fn of this.undo.splice(0)) fn()
  }

  private race(p: Promise<void>): Promise<void> {
    if (this.aborted) return Promise.resolve()
    return new Promise((resolve) => {
      const wake = () => resolve()
      this.wakers.add(wake)
      void p.then(() => {
        this.wakers.delete(wake)
        resolve()
      })
    })
  }
}

// One stage per ring (keyed by its <svg>): a new reset on the same ring replaces the old one.
const stages = new WeakMap<Element, Stage>()

/** Stops whatever runs on this ring and starts a fresh stage. */
export function begin(svg: Element): Stage {
  stopStage(svg)
  const st = new Stage()
  stages.set(svg, st)
  return st
}

/** Stops and clears the ring's running animation, if any (unlock, safety timeout). */
export function stopStage(svg: Element | null) {
  if (!svg) return
  const st = stages.get(svg)
  if (!st) return
  stages.delete(svg)
  st.stop()
  st.clear()
}
