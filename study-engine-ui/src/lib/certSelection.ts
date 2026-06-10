// Persistence for the selected cert, mirroring dashboardHelp.ts: a tiny pair
// of load/save functions over localStorage so a bank choice survives reloads
// instead of snapping back to the first bank in the list.

const STORAGE_KEY = 'study-engine-selected-cert'

/** The last cert the user selected, or null if none stored. */
export function loadSelectedCert(): string | null {
  return localStorage.getItem(STORAGE_KEY)
}

export function saveSelectedCert(cert: string): void {
  localStorage.setItem(STORAGE_KEY, cert)
}
