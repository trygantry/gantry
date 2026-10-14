# Gantry Design System & Visual Posture

This document defines the visual standards for Gantry's desktop UI. All styling must align with the "launch control room" posture: flat, operator-grade visual discipline, high data density, and strict semantic color usage.

## Visual Posture
The interface must feel like a professional launch operations console. It prioritizes clarity, data density, and stability.
- **Flatter Tonal Surfaces**: Replace glassmorphism (`backdrop-filter: blur()`), radial background gradients, and ambient glows with flat, solid, high-contrast dark backgrounds.
- **Hairline Borders**: Separate UI zones and panels using crisp, single-pixel hairline borders (`1px solid`).
- **Restrained Motion**: Remove decorative hover-scaling, floating drag clones with heavy glows, and sliding transitions in core layout components. Transitions should be direct and functional.
- **Reduced Radii**: Standardize border-radius to a range between `4px` and `8px` (down from rounded `12px`/`16px` cards).

## Color System
Color must never be decorative. It is a functional token reserved to convey launch readiness meaning.

### Semantic Status Scale
- **Go / Ship**: Green (`#10b981`). Used only for passed checks, completed sign-offs, and readiness status of Go.
- **At Risk / Warning**: Orange/Yellow (`#f59e0b`). Used for incomplete non-blocking items, stale reviews, and warning states.
- **No-Go / Blocker**: Red (`#ef4444`). Used for P0/P1 active blockers, failed gates, validation errors, and critical blocks.
- **Info / Interactive**: Blue (`#38bdf8` or `#3b82f6`). Used for timeline indicators, update messages, and informative badges.
- **Muted / Inactive**: Gray/Slate (`#6b7280`). Used for inactive, unconfigured, or disabled states.

### Core Surface Colors
- **App Canvas Background**: `#080b11` (Solid Dark)
- **Sidebar Background**: `#0c0f16` (Solid Sidebar Surface)
- **Card / Control Background**: `#161b22` (Solid Card Surface)
- **Panel Borders**: `#30363d` (Hairline Border)
- **Interactive Focus / Selection**: `#8b949e` (Neutral Highlighting)
- **Primary Text**: `#f0f6fc` (High Contrast White)
- **Secondary Text**: `#8b949e` (Medium Contrast Muted Gray)
- **Muted Text**: `#484f58` (Low Contrast Slate)

## Typography
Use modern, clean sans-serif typography for UI layouts (`Inter`), monospace for telemetry values/code paths (`JetBrains Mono`), and Outfit for section headers. Headers must never have decorative color gradients; keep headings in solid primary text color.

## Density Rules
Gantry is a workspace for active releases. Maintain a dense grid layout with minimal padding (`10px` to `16px` standard padding in lists/rows, `20px` to `24px` in larger views) so that the maximum amount of evidence is visible on screen without scrolling.

## Accessibility & Focus States
- **Accessible Contrast**: Ensure text elements on dark backgrounds maintain a contrast ratio of at least 4.5:1.
- **Visible Focus Indicators**: Do not disable focus rings on interactive components (`button`, `a`, `select`, `input`). Apply a clear, solid high-contrast border or outline (`2px solid var(--border-focus)`) when focused via keyboard navigation.
- **No Emoji-Only Statuses**: Every status indicator must have accessible text equivalents (e.g. "GO" next to a green marker, rather than a standalone emoji).
