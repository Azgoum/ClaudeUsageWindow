// The prototype's spring curve (CSS linear()), also usable from JS via motion's `animate`.
export const SPRING =
  'linear(0, 0.009, 0.035 2.1%, 0.141, 0.281 6.7%, 0.723 12.9%, 0.938 16.7%, 1.017, 1.077, 1.121, 1.149 24.3%, 1.159, 1.163, 1.161, 1.154 29.9%, 1.129 32.8%, 1.051 39.6%, 1.017 43.1%, 0.991, 0.977 51%, 0.974 53.8%, 0.975 57.1%, 0.997 69.8%, 1.003 76.9%, 1.004 83.8%, 1)'

/** Turns a CSS `linear(...)` easing into a JS easing function (same rules as CSS). */
export function linearEasing(spec: string): (t: number) => number {
  const body = spec.trim().replace(/^linear\(/, '').replace(/\)$/, '')
  const pts = body.split(',').map((s) => {
    const [v, p] = s.trim().split(/\s+/)
    return { y: Number(v), x: p === undefined ? null : Number(p.replace('%', '')) / 100 }
  })
  if (pts[0].x === null) pts[0].x = 0
  const last = pts[pts.length - 1]
  if (last.x === null) last.x = 1
  // Positions never go backwards; unpositioned runs are spread evenly between known ones.
  for (let i = 1; i < pts.length; i++) {
    if (pts[i].x !== null) pts[i].x = Math.max(pts[i].x!, pts[i - 1].x ?? 0)
  }
  for (let i = 1; i < pts.length; i++) {
    if (pts[i].x !== null) continue
    let j = i
    while (pts[j].x === null) j++
    const a = pts[i - 1].x!
    const b = pts[j].x!
    for (let k = i; k < j; k++) pts[k].x = a + ((b - a) * (k - i + 1)) / (j - i + 1)
  }
  const xs = pts.map((p) => p.x!)
  const ys = pts.map((p) => p.y)
  return (t) => {
    if (t <= 0) return ys[0]
    if (t >= 1) return ys[ys.length - 1]
    let i = 1
    while (i < xs.length - 1 && xs[i] < t) i++
    const x0 = xs[i - 1]
    const x1 = xs[i]
    return x1 === x0 ? ys[i] : ys[i - 1] + ((ys[i] - ys[i - 1]) * (t - x0)) / (x1 - x0)
  }
}

export const springEase = linearEasing(SPRING)
