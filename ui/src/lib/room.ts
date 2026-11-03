// Extra window height (CSS px) an open popup needs beyond the widget box. The window grows by
// this much while the popup is open (see bridge.fitWindow); 6 px matches the collision padding.
const PAD = 6

/** Ring tooltip: placed 16 px under the ring trigger (`triggerBottom`, relative to the widget top). */
export const tipRoom = (triggerBottom: number, tipH: number, widgetH: number): number =>
  Math.max(0, Math.ceil(triggerBottom + 16 + tipH + PAD - widgetH))

/** Context menu: opens at the cursor (`cursorY`, relative to the widget top) and goes down. */
export const menuRoom = (cursorY: number, menuH: number, widgetH: number): number =>
  Math.max(0, Math.ceil(cursorY + menuH + PAD - widgetH))
