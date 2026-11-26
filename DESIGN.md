# GumballZ Get Design System

## Visual language

A neo-brutalist desktop workbench organized as asymmetric Bento panels. Keep the download URL as the dominant action, group related controls, and reserve generous 16px gaps between cards. Preserve the platform's existing navigation and accessibility patterns.

## Tokens

### Dark (default)
- Canvas `#090D16`; card `#111827`; secondary surface `#1E293B`.
- Electric Cyan `#22D3EE`; Acid Lime `#A3E635`; Donate Amber `#FBBF24`.
- Primary text `#F8FAFC`; muted text `#94A3B8`; ink border `#E2E8F0`.
- Hard card shadow `4px 4px 0 #000000`; cyan interactive accent shadow `4px 4px 0 #06B6D4`.

### Light
- Canvas `#F8FAFC`; card `#FFFFFF`; Electric Cyan `#06B6D4`; Warm Amber `#F59E0B`.
- Ink `#0F172A`; hard shadow `4px 4px 0 #0F172A`.

### Shape and interaction
- Bento cards use a 2px solid ink border, 12px radius, and hard zero-blur offset shadow.
- Buttons, fields, and status chips use 8px radius. URL workspace panels may use 12px.
- Pressed controls translate by 2px and reduce shadow to 2px 2px.
- Use clear focus rings and keyboard-operable controls. Never communicate status by color alone.

### Type
- Space Grotesk or Plus Jakarta Sans for bold geometric headings.
- Inter for body copy; JetBrains Mono for URLs, bitrates, digests, and hotkeys.

## Layout

Dashboard, queue, and tools use modular asymmetric grids with 16px gaps. At narrow widths, collapse to one column without horizontal scrolling. Keep the URL input, active download summary, and pinned tools visible near the top of the Home workspace.

## Brand

Use the GumballZ cat-G mark from `src/lib/ui/BrandMark.svelte` (light-blue cat face forming a G, with an amber download arrow). About credits Nguyen Quoc Hung (GumballZ). Do not use upstream mascot art as product branding.

## Backend boundary

UI changes must preserve existing Rust `invoke(...)` names and argument contracts.
