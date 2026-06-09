import '@testing-library/jest-dom/vitest'
import '@testing-library/svelte/vitest'

// jsdom does not implement the Web Animations API. Svelte 5 transitions use
// element.animate() internally, so we stub it to avoid test failures.
Element.prototype.animate = function () {
  return { onfinish: null, cancel() {}, finish() {}, play() {}, pause() {} } as unknown as Animation
}
