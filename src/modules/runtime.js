/**
 * Native transport boundary.
 *
 * Domain adapters decide when demo data is permitted. This module only exposes
 * the real Tauri transport and never substitutes sample data after a native
 * failure.
 */
export const hasTauri = typeof window !== 'undefined' && window.__TAURI__ !== undefined;

export function invokeNative(command, args) {
  if (!hasTauri) throw new Error('Tauri API is not available');
  return window.__TAURI__.core.invoke(command, args);
}
