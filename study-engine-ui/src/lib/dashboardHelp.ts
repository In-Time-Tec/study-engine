// Persistence for the dashboard "Operation manual" panel.
//
// The panel starts collapsed to keep the dashboard uncluttered; a user who
// opens it has that remembered across reloads, mirroring theme.ts: a tiny pair
// of load/save functions over localStorage, kept here as pure logic rather than
// in the view so the Humble View components stay free of stored state.

const STORAGE_KEY = 'study-engine-help-collapsed'

/** Whether the "Operation manual" panel is collapsed. Defaults to collapsed
 *  (true) when nothing is stored. */
export function loadHelpCollapsed(): boolean {
  return localStorage.getItem(STORAGE_KEY) !== 'false'
}

export function saveHelpCollapsed(collapsed: boolean): void {
  localStorage.setItem(STORAGE_KEY, String(collapsed))
}
