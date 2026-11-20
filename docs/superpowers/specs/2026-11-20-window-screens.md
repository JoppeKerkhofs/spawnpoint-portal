# Spawnpoint Portal: the window's screens

20 November 2026. What each screen of the window looks like and does, for plan 5 (Tasks 4 and 5).
It replaces the screens file named in plan 5's Task 1.

The design is ten boards of 1100 by 720, saved byte for byte in `window-design/`:

| View | Standard theme | Blocky theme |
| --- | --- | --- |
| Servers | `PortalServers.dc.html` | `PortalBlockyServers.dc.html` |
| Instances | `PortalInstances.dc.html` | `PortalBlockyInstances.dc.html` |
| One instance | `PortalInstance.dc.html` | `PortalBlockyInstance.dc.html` |
| Panels | `PortalPanels.dc.html` | `PortalBlockyPanels.dc.html` |
| Account | `PortalAccount.dc.html` | `PortalBlockyAccount.dc.html` |

Each board is one HTML file: a `<helmet><style>` block (identical in all five boards of a theme),
the markup with `{{ }}` placeholders, and a `<script type="text/x-dc">` whose `renderVals()` holds
the sample data and switches the states. A board whose script declares `listState` in `data-props`
has a state picker; other states are reached by clicking in the board (local `setState`). The boards
do not run outside the design canvas (they load `./support.js`); read them as source.

Where a board and the panel as implemented (`../spawnpoint/panel`, branch `main`, version 1.7.0)
differ, the panel wins for every token and every piece the two share; the boards decide everything
the panel has no counterpart for (the sidebar, the status bar, the launcher's own pieces). Each
choice is recorded below with its source. "Board" means the Portal board; "panel" means the panel's
code.

## 1. Shell

### Window and title bar

- One window, 1100 by 720, minimum 900 by 600 (plan 5, Task 2).
- The boards draw their own title bar, so the native one goes: `decorations: false` in
  `tauri.conf.json`, the bar marked `data-tauri-drag-region`, and the window permissions
  `core:window:allow-minimize`, `core:window:allow-toggle-maximize`, `core:window:allow-close` and
  `core:window:allow-start-dragging` added to `capabilities/default.json`. Double-clicking the
  drag region toggles maximise. (Windows 11's snap-layout flyout on the maximise button is lost
  with a custom bar.)
- Title bar: 32px high, full width, `display:flex; align-items:center; gap:8px; padding-left:12px`;
  12px text. Left: the pixel block logo at 16 by 16, then "Spawnpoint Portal" (flex 1). Right: three
  buttons, each 46 by 32, no border, transparent, icon 10 by 10 drawn with a 1px stroke:
  Minimize (`M0 5h10`), Maximize (a 9 by 9 square), Close (two diagonals). `aria-label`
  "Minimize", "Maximize", "Close". Hover: `rgba(255,255,255,.12)` (blocky `.14`); Close hover
  `#c42b1c`. Colours per theme in the token table (title bar row).
- The logo is the panel's `panel/public/favicon.svg` (16 by 16 viewBox, `shape-rendering:
  crispEdges`, 109 rects); the boards inline the same drawing. Copy the file and use it at 16px
  (title bar), 26px (sidebar, blocky 30px), 30px (Panels rows) and 56px (empty states).

### Layout

```
+------------------------------------------------------------+
| title bar 32px                                             |
+------------+-----------------------------------------------+
| sidebar    | head (fixed): h1, summary, actions            |
| 212px      +-----------------------------------------------+
| (fixed)    | content (scrolls: overflow:auto)              |
|            |                                               |
+------------+-----------------------------------------------+
| status bar 44px, full window width (fixed)                 |
+------------------------------------------------------------+
```

- `.win` is a column: title bar, body (`flex:1; min-height:0; display:flex`), status bar.
- Sidebar: 212px, `padding:14px 10px`, column, `gap:4px` (blocky `6px`), right border (token
  table). Never scrolls.
- Main: column, `flex:1; min-width:0`.
  - Head: `padding:20px 24px 14px; display:flex; align-items:flex-end; gap:12px; flex-wrap:wrap`.
    Left block (flex 1, `min-width:0`): h1 and the summary line under it. Right: the view's
    buttons.
  - Content: `flex:1; min-height:0; overflow:auto; padding:0 24px 20px; display:flex;
    flex-direction:column; gap:12px` (blocky `14px`). Only this scrolls. Cards inside it are
    `flex:0 0 auto` so they keep their height and the content scrolls.
- Status bar: 44px, `padding:0 16px; gap:12px`, 13px text. Always present, in every view.
- Dialogs: a backdrop over everything below the title bar (`inset:32px 0 0 0`), the dialog
  centred. See Components.

### Navigation

- In the sidebar, top to bottom:
  1. Brand row: logo 26px (blocky 30px), "Spawnpoint Portal"; `gap:10px; padding:0 8px 14px`
     (blocky `0 4px 12px`); 15px weight 700 (blocky: Minecraft font 16px weight 400,
     `text-shadow:2px 2px 0 #000`, white). Not a link.
  2. Four items, in this order, each an 18 by 18 stroke icon (24 viewBox, stroke 2, round caps
     and joins) and a label:

     | Label | Icon paths in the boards | Lucide equivalent |
     | --- | --- | --- |
     | Servers | `M4 5h16v5H4zM4 14h16v5H4z`, `M8 7.5h.01M8 16.5h.01` | `server` |
     | Instances | `M12 3 4 7.5v9L12 21l8-4.5v-9z`, `M4 7.5 12 12l8-4.5M12 12v9` | `box` |
     | Panels | the two-link chain (`M10 14a4 4 0 0 0 5.7 0l3-3…`) | `link` |
     | Account | circle r4 at 12,8 and `M4 21c1-4 4.5-6 8-6s7 2 8 6` | `user` |

  3. Footer (pushed down with `margin-top:auto`): a link to Account. `padding:10px 8px 0`
     (blocky `10px 4px 0`), top border (token table: divider; blocky `3px solid #111`). Avatar
     32px, then two lines: the player name, weight 700 (Minecraft font in blocky), and "Offline
     name", 12px muted.
     - Standard avatar: a 32px circle, `#e3e6e9` (dark: `#1e2226`), the initials "JK" 12px weight
       700 `#1f2328` (dark: `#e8eaed`). The boards show "JK" for "JoppeK"; the rule is not
       stated. Use: the capital letters of the name, at most two; when there are none, its first
       character upper-cased.
     - Blocky avatar: a 32px square with a 2px `#111` border drawn as a Minecraft face in CSS
       (`.avatar` in the blocky boards: skin `#c99a6b`, hair `#3b2614` top 35%, eyes `#1b1b1b`);
       no text (`font-size:0`). It is the same for every player (skins are a non-goal).
- Items are `<button>`s in `<nav aria-label="Main">` (plan 5, Task 4), with
  `aria-current="page"` on the active one. Each: `min-height:40px; padding:0 12px; gap:10px;
  radius 6px` (blocky 0).
  - Standard: rest transparent, text = foreground. Hover: background accent (`#f6f6f6`, dark
    `#1e2226` at 50%). Active: background primary (`#1f2328`, dark `#e8eaed`), text
    primary-foreground, weight 600. (Board hover is `#f1f2f3`; the panel's ghost hover is the
    accent, `#f6f6f6`.)
  - Blocky (board, matches the panel's blocky header nav): background `#4a4a4a`, text `#f0f0f0`,
    `border:2px solid #111`, bevel `inset -3px -4px 0 rgba(0,0,0,.35), inset 3px 3px 0
    rgba(255,255,255,.3)`, `text-shadow:1px 1px 0 #000`. Hover `filter:brightness(1.15)`. Active:
    background `#3c8527`, white, weight 700.
- The One instance view is not in the sidebar: it opens from Instances, Instances stays the
  active item, and the view has a back link "Instances" at its top.
- Start view: Servers.

### Where progress and errors appear

- Progress of a join or a launch: only in the status bar (see Progress). One operation at a time.
- Errors of an operation: the status bar turns to its failed state with the error and a Close
  button.
- Errors of loading a view: inside that view's content, as a note or a failed row (each view
  below).
- No toasts. The panel has `ui/sonner`; the window does not use it.

### Themes and modes

- Two themes, as in the panel: standard and blocky.
  - Standard has light and dark, like the panel (`useAppearance.ts`): `light`, `dark` or
    `system`, default `system`, stored in `localStorage` under `appearance`; `system` follows
    `prefers-color-scheme` and updates when it changes; dark is the class `dark` on `<html>`.
    The Portal boards draw light only; dark uses the panel's `.dark` tokens, and the colours the
    panel does not have (notes, badges, status bar failed) are derived in the token table and
    marked so.
  - Blocky has one look and never takes `dark` (panel `updateTheme` returns early for it). It is
    `data-theme="blocky"` on `<html>`; standard is `data-theme="default"`.
- The panel picks its theme from the server environment (`PANEL_THEME`); the launcher has no such
  thing, so it needs its own setting. The design puts no theme or appearance control anywhere.
  Smallest honouring: store `theme` (`default` or `blocky`, default `default`) in `localStorage`
  beside `appearance`, apply both to `<html>` in `src/main.ts` before `app.mount`, and offer them
  in Account (see Open questions). Window-only preferences, so no core change.

## 2. Tokens

Columns: standard light, standard dark, blocky. "Source" says what was followed. Panel names are
the CSS variables in `panel/resources/css/app.css` (`:root`, `.dark`, `html[data-theme='blocky']`,
and the blocky in-card scope `html[data-theme='blocky'] :is(.bg-card, …)`, written "blocky card"
below).

### Colours

| Token | Light | Dark | Blocky | Source; difference |
| --- | --- | --- | --- | --- |
| Page background | `#f1f2f3` | `#0e1012` | `#232325` with `repeating-conic-gradient(rgba(255,255,255,.025) 0 25%, transparent 0 50%)` at `24px 24px` | panel `--background`, blocky `body`; same in boards |
| Text | `#212529` | `#d5d9dd` | `#f0f0f0` outside cards; `#2b2b2b` inside cards and dialogs | panel `--foreground`, blocky card `--foreground`; same |
| Heading text (h1, card titles) | `#1f2328` | `#e8eaed` | h1 `#ffffff`; card head title `#ffffff`; title inside a card body or dialog `#1b1b1b` | panel `--primary` (light/dark); board for blocky |
| Muted text | `#5f666d` | `#9aa2aa` | `#d0d0d0` outside cards; `#3d3d3d` inside | panel `--muted-foreground`; board blocky uses `#cfcfcf`/`#c4c4c4` outside cards: follow the panel's `#d0d0d0` |
| Surface (card, sidebar, status bar) | `#ffffff` | `#171a1e` | cards `#c6c6c6`; sidebar and status bar `#2b2b2b` | panel `--card`, blocky `--card` and `--header`; same |
| Dialog surface | `#f1f2f3` | `#0e1012` | `#c6c6c6` | panel `DialogContent` (`bg-background`); board uses `#ffffff` |
| Card head background | `#f6f6f6` | `#1e2226` | `#8b8b8b` (board; see section 6, use `#6e6e6e`) | panel `--muted`; same in light |
| Primary (filled button, active nav) | `#1f2328` on which `#ffffff` | `#e8eaed` on which `#111316` | `#3c8527` on which `#ffffff`, `text-shadow:1px 1px 0 #000` | panel `--primary`/`--primary-foreground`, blocky `[data-slot=button].bg-primary`; same |
| Primary hover | primary at 90% opacity | same | `filter:brightness(1.15)` | panel `hover:bg-primary/90`, blocky button hover; board `#3d434a` and `brightness(1.1)` |
| Secondary button | background `#f1f2f3`, border `#1f2328`, text `#1f2328`; hover `#f6f6f6` | background `#353a40` at 30%, border `#353a40`, text `#e8eaed` | `#dcdcdc`, text `#2b2b2b`, `2px solid #111` | panel `outline` variant with `border-primary` (as in `Dashboard.vue`, `AppDialog.vue`); board fills it `#ffffff` |
| Destructive text and outline | `#a12a2f` | `#ff9a9a` | `#8a1f1b` inside cards; `#ff8f88` outside | panel `--destructive`, blocky and blocky card; same |
| Destructive fill | `#a12a2f`, text `#ffffff` | `#ff9a9a`, text `#111316` | `#b3312c`, text `#ffffff`, `text-shadow:1px 1px 0 #000` | panel `destructive` variant; same |
| Border (cards, sidebar, status bar) | `rgba(0,0,0,.175)` 1px | `rgba(255,255,255,.12)` 1px | `#111` 3px (cards, dialogs, notes), 4px (sidebar right, status bar top), 2px (buttons, inputs, pills, badges) | panel `--border`, blocky rules; same |
| Divider (rows, card head bottom, sidebar footer) | `#e6e6e6` | `#2a2f35` | `#8b8b8b` 1px | panel `--divider`; board rows `#eceef0` (light) and `3px solid #a8a8a8` (blocky) |
| Input border | `#dee2e6` | `#353a40` | `#111` 2px | panel `--input`; board `#c9ced3`; see section 6 |
| Input background | `#ffffff` | `#353a40` at 30% over the surface | `#3a3a3a`, text `#ffffff`, placeholder `#bdbdbd`, `inset 3px 3px 0 rgba(0,0,0,.4), inset -3px -3px 0 rgba(255,255,255,.12)` | panel `Input.vue` (`bg-card`, `dark:bg-input/30`), blocky inputs; board omits the second inset |
| Focus outline | `2px solid #3fa36b`, offset 2px | same | `3px solid #ffffff`, offset 2px; inside cards and dialogs `3px solid #111111` | board; panel uses a 3px ring of `--ring` at 50% (`#8a9097`), which fails 3:1 on white; blocky inside cards is a fix, see section 6 |
| Brand green | `#3fa36b`; bright `#5fd08d` | same | `#7fd35a` (progress fill), `#3c8527` (primary) | panel `--color-brand`, `--color-brand-bright`; same |
| Title bar | `#1f2328`, text `#ffffff` | `#0a0c0e`, text `#ffffff` | `#111111`, text `#f0f0f0` | panel `--header`; board for blocky (panel's blocky `--header` is `#2b2b2b`, `--header-border` `#111`) |
| Dialog backdrop | `rgba(0,0,0,.8)` | same | same | panel `DialogOverlay` (`bg-black/80`); board `rgba(20,22,24,.45)`, blocky `rgba(0,0,0,.6)` |
| Status bar, failed | `#fbe3e3`; icon `#a12a2f` | `#3d1c1e`; icon `#ff9a9a` (derived from the error pill) | `#5a1c1a`; icon `#ffb3ae` | board; dark derived |
| Status dot | idle `#9aa2aa`, playing `#3fa36b`; 8px circle | same | same colours; 10px square, `2px solid #111` | board |
| Progress bar | track `#e6e6e6`, fill `#3fa36b`, 6px, radius 999px | track `#2a2f35`, fill `#3fa36b` | track `#2a2a2a`, `2px solid #111`, 14px; segments every 21px (`transparent 0 18px, rgba(0,0,0,.45) 18px 21px`); fill `#7fd35a` with a moving highlight (`linear-gradient(90deg, transparent, rgba(255,255,255,.55), transparent)` at `120px 100%`, `mc-shimmer 2.6s linear infinite`) | track: panel `--divider` (board `#e3e6e9`); blocky: panel `.mc-meter` (board segments 16+3px and highlight 100px) |
| Skeleton | `#f6f6f6` | `#1e2226` | `#b0b0b0` | panel `ModSkeleton.vue` (`bg-muted`); board `#e3e6e9` and `#a8a8a8` |

State pills (panel `StatusPill.vue`; the boards have the same light colours without the mark):

| Tone | Used for | Light (bg / text) | Dark | Blocky (bg / text) |
| --- | --- | --- | --- | --- |
| ok | Running, Game running | `#e1f1e7` / `#1d6b3f` | `#173323` / `#7fd6a2` | `#3c8527` / `#ffffff` |
| busy | Installing | `#fbefd5` / `#7a4f00` | `#3a2d12` / `#f0c36b` | `#e07b1a` / `#231200` |
| idle | Offline | `#eceef1` / `#4f565d` | `#2a2f35` / `#a9b0b7` | `#5a5a5a` / `#ffffff` |
| error | Error | `#fbe3e3` / `#a12a2f` | `#3d1c1e` / `#ff9a9a` | `#b3312c` / `#ffffff` |

Kind badges (board only; the panel has no such badge):

| Badge | Light (bg / text) | Dark (derived from the pill tones) | Blocky (bg / text) |
| --- | --- | --- | --- |
| Own, Server | `#e1f1e7` / `#1d6b3f` | `#173323` / `#7fd6a2` | `#3c8527` / `#ffffff` |
| Pack, Offline | `#eceef1` / `#3d434a` | `#2a2f35` / `#a9b0b7` | `#5a5a5a` / `#ffffff` |
| By hand, Coming later | `#f1ece0` / `#5e4a1c` | `#3a2d12` / `#f0c36b` | `#a7804b` / `#1b1205` |

Notes (board only; the dark values are derived from the pill tones and the dark border):

| Note | Light | Dark | Blocky |
| --- | --- | --- | --- |
| warn | bg `#fdf7e9`, border `1px #e6c98a`, text `#3d2a00` | bg `#3a2d12`, border `rgba(255,255,255,.12)`, text `#f0c36b` | bg `#f3d98b`, `3px #111`, text `#2b1d00` |
| err | bg `#fbe3e3`, border `1px #e9b4b4`, text `#5a1518` | bg `#3d1c1e`, border `rgba(255,255,255,.12)`, text `#ff9a9a` | bg `#f2b6b3`, `3px #111`, text `#3a0c0a` |
| info | the card surface and border | the card surface and border | `#c6c6c6`, `3px #111`, text `#2b2b2b` |
| ok icon | `#1d6b3f` | `#7fd6a2` | `#1d5a10` |

Server and instance tiles (standard theme, board only): a 32px square, radius 6px, the name's
first character 13px weight 700, colours by the block texture the blocky theme would draw (see
Components, "Block cube"): grass `#e1f1e7`/`#1d6b3f`, wood `#e9e0d2`/`#4a3417`, sand
`#f3eccd`/`#5e4a1c`, machine `#e3e6e9`/`#3d434a`, stone `#eceef1`/`#3d434a`, wool
`#ece3f3`/`#5b2f7a`, chest `#f1ece0`/`#5e4a1c`; a server in the error state `#fbe3e3`/`#a12a2f`
(my reading of the "Skyblock test" sample). Dark: `#1e2226`/`#e8eaed` for all (derived; not
designed).

### Type

| Token | Standard | Blocky | Source; difference |
| --- | --- | --- | --- |
| Body family | `system-ui, -apple-system, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif` (Segoe UI on Windows) | `'Ubuntu', system-ui, 'Segoe UI', sans-serif` | panel `--font-sans`, blocky `body`; board lacks Helvetica, Arial (no effect on Windows) |
| Heading family (h1, h2, brand, server, panel and player names: class `mc-name` in the boards) | body family | `'Minecraft', 'Ubuntu', system-ui, sans-serif`, weight 400, `letter-spacing:.01em`, `font-synthesis:none` | panel blocky heading rule; board `.02em` |
| Mono (addresses, hosts, file names, code) | Tailwind's `font-mono` stack (Consolas on Windows) | `'Ubuntu Mono', ui-monospace, Menlo, Consolas, monospace` | panel; board standard `ui-monospace,'Cascadia Mono',Consolas` (Cascadia is missing on some Windows 10) |
| Body size | 14px, line-height 1.45 | same | board (the panel's pages are 16px; the window keeps the board's density) |
| Small | 12px | same | board |
| Mono size | 13px | 15px (Ubuntu Mono runs small) | board |
| h1 | 24px, line-height 1.3, weight 700 | 24px, weight 400, line-height 1.3, `text-shadow:3px 3px 0 #000`, white | panel `text-2xl leading-[1.3] font-bold`; board 22px/1.25 (blocky 26px/1.2); the panel shadows a blocky h1 only inside its sky, the board shadows it always: keep the board's shadow |
| Summary under h1 | 15px, muted, `margin-top:4px` | same | panel `mt-1 text-[0.9375rem]`; board 13px |
| Card title (h2) | 16px, weight 700 | 16px, weight 400 | panel `SectionCard.vue`; board 15px |
| Dialog title | 18px, weight 600, line-height 1 | 18px, weight 400 | panel `DialogTitle.vue`; board 17px 700 |
| Labels | 14px, weight 500 | 14px, weight 500, `#1b1b1b` | panel `Label.vue`; board 13px 600 |
| Buttons | 14px, weight 500 | same | panel `buttonVariants` (`text-sm font-medium`); board 13px 600 (blocky 700) |
| Pills | 12px, weight 600, uppercase, `letter-spacing:.02em` | 15px, weight 600 (falls to Ubuntu Bold), `text-transform:capitalize`, no letter-spacing | panel `StatusPill.vue` and `.status-pill` blocky rule; board 11px (blocky 12px 700) |
| Badges | 11px, weight 600 | 11px, weight 700 | board |
| Pairing code | 46px, weight 700, `letter-spacing:.14em`, line-height 1, mono | 52px, Ubuntu Mono | board |
| Numbers that change (counts, sizes, timers) | `font-variant-numeric: tabular-nums` | same | board and panel |

Fonts to bundle. The window must paint before any network call finishes, and the CSP
(`default-src 'self'`) blocks Google Fonts anyway, so every font is a local file under
`src/assets/fonts/`, declared with `@font-face` (`font-display: swap`) in `src/style.css`. A face
is only read from disk when text uses it, so the blocky files cost nothing in the standard theme.

| File | Use | From |
| --- | --- | --- |
| `Minecraft.ttf` (14,488 bytes) | blocky headings and names | `../spawnpoint/panel/public/fonts/Minecraft.ttf` ("Minecraft" by Craftron Gaming; check its licence covers handing the launcher to friends) |
| `Ubuntu-Regular`, `Ubuntu-Medium`, `Ubuntu-Bold` (400, 500, 700) | blocky body, buttons, bold text | Ubuntu Font Family (Ubuntu Font Licence 1.0); `.woff2`, Latin subset is enough |
| `UbuntuMono-Regular`, `UbuntuMono-Bold` (400, 700) | blocky mono, pairing code | same |

The standard theme uses system fonts only; nothing to bundle. The panel also loads "Instrument
Sans" through `vite.config.ts`, but `--font-sans` overrides it, so it is unused: do not copy it.
Weight 600 in the blocky theme falls to Ubuntu Bold.

### Radii, spacing, shadows

| Token | Standard | Blocky | Source; difference |
| --- | --- | --- | --- |
| Radius | 6px (`--radius: .375rem`) for cards, buttons, inputs, selects, nav items, tiles, notes, dialogs; 4px checkbox and badges; 999px pills and progress bar; 50% avatar and dots | 0 everywhere (`border-radius:0 !important`), except the round mark inside a pill | panel; board dialog 8px |
| Spacing | 4px base. Used values: 2, 3, 4, 6, 8, 10, 12, 14, 16, 20, 24, 28, 48 | same | board; card heads and rows use 16px sides (panel `px-4`) instead of the board's 14px |
| Card head | `padding:10px 16px; gap:10px`, bottom divider | `padding:9px 16px`, `border-bottom:3px solid #111`, `text-shadow:1px 1px 0 #333` | board, sides from the panel |
| Row | `padding:10px 16px; gap:14px; min-height:40px`, top divider (none after a card head) | same, divider per token table | board |
| Card body | `padding:14px` | same | board |
| Shadow, dialog | `0 10px 15px -3px rgb(0 0 0/.1), 0 4px 6px -4px rgb(0 0 0/.1)` | bevel `inset -4px -4px 0 rgba(0,0,0,.28), inset 4px 4px 0 rgba(255,255,255,.6)` | panel `shadow-lg`; board `0 12px 40px rgba(0,0,0,.3)` |
| Shadow, secondary button | `0 1px 2px 0 rgb(0 0 0/.05)` | bevel `inset -3px -4px 0 rgba(0,0,0,.35), inset 3px 3px 0 rgba(255,255,255,.35)`; pressed: `translateY(2px)` and `inset 3px 3px 0 rgba(0,0,0,.4)` | panel `shadow-xs`, blocky buttons; same bevel in boards |
| Shadow, cards and dialogs | none | bevel `inset -4px -4px 0 rgba(0,0,0,.28), inset 4px 4px 0 rgba(255,255,255,.6)` | panel blocky card rule; same |
| Motion | spinner 0.8s; skeleton Tailwind `animate-pulse` | cube spin 9s, bob 2.6s by 7px, shimmer 2.6s | panel; board spin 8s, bob 6px. Everything stops under `prefers-reduced-motion: reduce` in both themes (board and panel do it for blocky; do it for standard too) |

## 3. Components

Copy what is needed from the panel (plan 5's rule); no shared package. Icons: copy the inline SVG
paths from the boards (24 viewBox, stroke 2, round caps and joins, `aria-hidden="true"`); they are
drawn in the style of Lucide, the panel's set (`@lucide/vue`), and nothing more is needed. Icons
used: refresh (`M20 12a8 8 0 1 1-2.3-5.6M20 4v5h-5`), plus, play (`M7 5v14l12-7z`), folder,
trash, upload (`M12 16V4M7 9l5-5 5 5M4 20h16`), lock, external link, check (`M4 12l5 5L20 6`),
warning triangle (`M12 3 2 20h20z` + `M12 10v4M12 17.5v.5`), info circle, chevron left
(`M15 5l-7 7 7 7`).

### Button

Panel `components/ui/button/index.ts`.

- Sizes: default 36px high, `padding:0 16px` (12px with an icon), gap 8px, icon 16px
  (board 34px and 15px icons); small 32px, `padding:0 12px` (10px with an icon), gap 6px, icon
  14px (board 30px). `white-space:nowrap`.
- Variants: primary, secondary (outline with primary border), danger (outline in destructive),
  danger fill. Colours per token table.
- Disabled: `opacity:.5`, no pointer events (panel); blocky `filter:grayscale(1) brightness(.8)`.
  (Board: `.45`.)
- A link that looks like a button ("Link a panel", "Go to Servers") is a `<button>` that switches
  the view; there are no URLs inside the window.

### Input and select

Panel `components/ui/input/Input.vue`, selects as in `Dashboard.vue` (`selectClass`). 36px high
(board 34px), `padding:0 12px` (select 10px), 14px text, border and background per token table;
blocky 36px with the bevel. Selects are native `<select>`. A label sits above with 4px between
(board `.label`), or is `sr-only` where the design shows none (mod search).

### Checkbox

Only one in the design (Panels, http consent). A native `<input type="checkbox">`, 16px,
`accent-color` = primary (blocky `#3c8527`), aligned to the first line of its label. The panel's
`ui/checkbox/Checkbox.vue` wraps `reka-ui`; not worth a dependency for one box.

### State pill

Panel `components/StatusPill.vue`, copied with its `data-tone` and blocky rules. Anatomy:
`inline-flex; gap:6px; padding:2px 9px` (blocky `1px 10px`, `2px solid #111`), a mark, the text.
The mark keeps state readable without colour: ok a filled 6px dot, idle a 6px ring (1.5px), busy a
10px ring with one side open that spins, error a "!" (10px). The boards' pills have no mark:
follow the panel.

### Kind badge

Board only. `font-size:11px; font-weight:600; padding:1px 6px; radius:4px` (blocky `0 6px`,
`2px solid #111`, weight 700). Texts in the design: "Server", "Pack", "By hand", "Own", "Offline",
"Coming later". Colours per token table. Kept square-cornered so a kind is never mistaken for a
state.

### Card and rows

- Card: surface, border, radius, `overflow:hidden`. Optional card head (title h2, then muted 12px
  meta, a spacer, actions). Rows separated by the divider.
- The Servers card puts several heads in one card (one per panel); a head after a row gets a top
  border too (board `.row + .card-head`).
- List row: `display:flex; align-items:center` with fixed-width columns (given per view). Text
  that may be long: `white-space:nowrap; overflow:hidden; text-overflow:ellipsis` with the full
  text in `title`.
- Panel counterpart: `SectionCard.vue` (card with heading) and the server cards in `Dashboard.vue`.

### Tile (standard) and block cube (blocky)

- Tile: see token table.
- Block cube: panel `components/BlockCube.vue` with the cube and texture classes from `app.css`
  (`.mc-cube-wrap`, `.mc-cube`, `.mc-face`, `.mc-f-*`, `.mc-t-*`, `.mc-bob`). Props `texture`,
  `size`, `spin`, `dim`. Add the two chest textures from the blocky boards (`.t-chest-top`,
  `.t-chest-side`), which the panel lacks. The panel's wool texture (`#e9e9e9`, 45 degree stripes)
  replaces the board's (`#ecebe4`, horizontal).
- Rules (panel `Dashboard.vue`): spin when the server is not offline; bob when running; dim
  (`grayscale(.75) brightness(.85)`) when not running. The board does not dim an installing
  server and uses `grayscale(.7) brightness(.9)`; follow the panel. Instance cubes in the list never spin or bob.
- Texture choice. The panel picks by server type, which the launcher does not receive. Smallest
  rule from what the launcher has: a pack (server with a modpack, or a pack instance) `machine`;
  loader `fabric` `wood`; `forge` `sand`; `neoforge` `machine`; no loader `grass`. The boards'
  sample textures (chest, stone, wool) were picked by hand; stone is used for a hand-made instance
  in the board ("Old times") and can stay for `Source::Manual` without a loader if wanted.

### Note

Board `.note`: `display:flex; gap:12px; align-items:flex-start; padding:12px 14px; radius 6px`,
a 20px icon, then the text block (bold first sentence, then the rest). Kinds warn, err, info
(colours in the token table). The panel's nearest is `ModsChangedNotice.vue` (a card with text
and a button).

### Dialog (confirmation)

Panel `components/AppDialog.vue` look; built on the native `<dialog>` element with `showModal()`
(focus is trapped, Esc cancels, the page behind is inert), not `reka-ui`. Width 448px (board
440px), `padding:24px` (board 20px), gap 16px (board 12px), surface and shadow per token table,
`role="alertdialog"`, `aria-labelledby` the title, `aria-describedby` the body. Title, one
paragraph (file names in bold mono), then the buttons right-aligned with 8px between: "Cancel"
(secondary) then the action (danger fill). Focus starts on Cancel. With `showModal()` the
title bar is inert while the dialog is open (the board leaves it usable); acceptable.

Plan 5 says Remove and Delete confirm with the dialog plugin's `confirm`; the design replaces that
with these in-window dialogs (their own wording and a red action button). The dialog plugin is then
needed only for `open` (picking a jar).

### Skeleton

Panel `components/ModSkeleton.vue`: `animate-pulse` blocks of the skeleton colour, radius 2px,
`motion-reduce:animate-none`, an `sr-only` "Loading…" with `role="status"`, the blocks
`aria-hidden`. Shapes per view below. (Board pulses to opacity .45 every 1.4s.)

### Empty state

Board `.empty`: a card, centred column, `gap:10px; padding:48px 24px` (blocky 40px), the logo at
56px, an h2 at 18px, one muted paragraph (`max-width:380px`, Instances 420px), one primary button.

### Spinner

Panel `components/ui/spinner/Spinner.vue` (Lucide `loader-2`, 16px, spinning). Board: a 14px ring
(standard) and a stepped square (blocky, `steps(8)`); follow the panel in both themes. In the
status bar it is `aria-hidden`.

### Progress bar

A `<progress>` element (plan 5, Task 4) styled as the token table says, `flex:1`, with an
`aria-label` naming the stage. Its value changes do not go in the live region (see section 6).

### Segmented filter

Board `.seg` (One instance): buttons with `aria-pressed`, in a group `role="group"
aria-label="Show"`. Standard: one 1px input-colour border around all, radius 6px, buttons 30px
high, `padding:0 10px`, 13px, separated by 1px borders; pressed = primary fill. Blocky: separate
buttons 4px apart, `2px solid #111`, `#dcdcdc`, bevel `inset -2px -3px 0 rgba(0,0,0,.3), inset
2px 2px 0 rgba(255,255,255,.35)`, pressed `#3c8527` white with shadow. The panel's
`AppearanceTabs.vue` is a similar control with another look; use the board's.

### Pairing code display

Board `.code`: the code in two groups of four with a space ("7ABR 547C"), mono, sizes in the type
table, `aria-hidden="true"`, followed by an `sr-only` span reading each character: "Code: 7 A B R 5
4 7 C".

### Status bar

See Progress.

### Blocky decorations

All in the blocky boards' style block and, where it exists, in the panel's `app.css`:
checkerboard page background; bevels on buttons, nav items, cards, dialogs and inputs; 2 to 4px
`#111` outlines; text shadows on light-on-dark text (`1px 1px 0 #000`, brand `2px 2px 0 #000`, h1
`3px 3px 0 #000`); block cubes; the drawn avatar; the segmented XP-bar progress. The blocky style
block also defines clouds, stars, particles, sparks, a lamp and more; the Portal boards use none
of these: do not copy them. The panel's sunset sky behind page headings (`BlockySky.vue`,
`.mc-sky`) is absent from the Portal boards (see Open questions).

## 4. Views

Shared head of every view: h1 (the view name), the summary line under it, actions on the right.
Shared status bar: see Progress. Text that comes from a panel or a pack is rendered as text,
never as HTML (plan 5, "Carried over").

### Servers

Board: `PortalServers.dc.html`, states by `listState`: "Filled", "Loading", "No panels linked",
"No panel answered". In Filled, the second panel is down until "Try again" is clicked; Join runs a
simulated progress (see Progress).

Head: h1 "Servers". Summary:

| State | Summary |
| --- | --- |
| Loading | "Asking your panels what they run…" |
| Filled, all answered | "{n} servers from {panels} panels · {running} running", e.g. "6 servers from 2 panels · 2 running" |
| Filled, some failed | "{n} servers from {answered} of {panels} panels · {running} running", e.g. "5 servers from 1 of 2 panels · 2 running" |
| No panels linked | "Nothing linked yet" |
| No panel answered | "0 of 2 panels answered" |

(The board counts running servers of the first panel only; count all.) Action: secondary button
with the refresh icon, "Refresh", disabled while loading. Calls `servers`. No timer.

Content by state:

- Loading (no earlier answer in this session): one card, `aria-busy="true"`: a head with a 160 by
  14 skeleton; three rows of a 32 by 32 block, a flexible 14px bar, a 90 by 14 bar, a 70 by 30
  block. With an earlier answer, keep showing it while the new one loads.
- No panels linked (`panels` is empty): empty state, h2 "No panels linked yet", text "Link a
  Spawnpoint panel and its servers appear here, ready to join with one click.", primary button
  with the link icon "Link a panel" (opens Panels).
- No panel answered (every entry of `servers` has an `error`): an err note, `role="alert"`: bold
  "No panel answered." then "{names} cannot be reached. Check your internet connection, then try
  again." with the panel names joined as "A and B" (sample: "Joppe’s Spawnpoint and Lien’s panel
  cannot be reached. Check your internet connection, then try again."); a small secondary button
  "Try again" (calls `servers`).
- Filled: one card. Per panel, in the order `servers` returns:
  - Head: h2 the panel name (`mc-name`), its address in mono muted (ellipsis), a spacer, muted
    small count: "{n} servers" / "1 server", or "not answering" when the panel failed.
  - When the panel failed: one row, `role="alert"`: a warning icon 18px in destructive, bold the
    reason, then muted "Its servers are hidden until it answers.", a small "Try again" (calls
    `servers`; there is no per-panel command, so it refreshes everything). The board's reason is
    "The panel cannot be reached."; the core gives an error string, shown as it comes (see Beyond
    the core).
  - Otherwise one row per server, columns left to right:
    1. Tile (standard) or block cube 26px in a 34px slot (blocky).
    2. Flexible: name 15px weight 700 (`mc-name`, ellipsis, `title`); under it the hostname, mono
       12px muted, ellipsis.
    3. 150px: state pill; under it, 12px muted, the players: "{online} of {max} online",
       "Nobody online" when 0, "—" when the panel sends none.
    4. 170px: "Minecraft {game_version}" weight 600; under it, 12px muted ellipsis, what runs:
       a pack "{Catalogue} pack {version_name}" ("Modrinth pack 1.3.7", "CurseForge pack 5.0";
       FTB "FTB pack …"), else the loader "{Loader} {version}" ("Fabric 0.20.1"), "{Loader}
       (newest)" when the version is null ("Forge (newest)"), or "No mod loader".
    5. 128px, right-aligned column: the primary Join button (`min-width:96px`) and under it a
       12px muted hint.
- Join button labels: "Join"; "Preparing…" while this server's sync runs; "Starting…" while its
  game starts; "Playing" while its game runs. `aria-label`: "Join {name}", or the label and the
  name ("Preparing… Survival").
- Join is disabled while any operation is preparing or starting, when the server's state is
  installing, and on the server whose game is playing. Joining another server while a game plays
  is allowed. Hints: "Already starting" on the server being prepared or started; "Server is off.
  You can still sync." on an offline server; "Still installing its pack." on an installing one;
  else empty. (Plan 5 allows joining any state; the design blocks "installing", where
  `client-mods` answers 409 anyway.)
- Join calls `join(panel_id, slug)` through `withProgress`. A non-empty answer (files to fetch by
  hand) opens that instance's view, which shows them.

Sample data (board): panel "Joppe’s Spawnpoint", `https://panel.joppek.be`, servers Survival
(`survival.mc.joppek.be`, running, 3 of 20, 26.3, Fabric 0.20.1), Main (`mc.joppek.be`, running, 7 of
20, 26.3, No mod loader), Homestead (`homestead.mc.joppek.be`, offline, Nobody online, 1.20.1,
Modrinth pack 1.3.7), All the Mods 10 (`atm10.mc.joppek.be`, installing, —, 1.21.1, CurseForge pack
5.0), Skyblock test (`skyblock.mc.joppek.be`, error, —, 26.3, Forge (newest)); panel "Lien’s panel",
`https://panel.lienb.be`, failed, or after Try again "Lien’s creative build" (`build.lienb.be`,
running, 1 of 8, 26.3, No mod loader). The board opens mid-join: Homestead, step 6 of 6, Mods, 142
of 366 files.

Blocky board differences: the tile is a block cube; the failed-row icon is `#8a1f1b`; everything
else is the token swap.

### Instances

Board: `PortalInstances.dc.html`, `listState`: "Filled", "Loading", "Empty". The form opens with
"New instance"; Play marks a row running.

Head: h1 "Instances". Summary: Loading "Reading your instances…"; Empty "Nothing here yet";
Filled "{n} instances · {k} couldn’t be read" (the second part only when k > 0; sample "6
instances · 1 couldn’t be read"). Action: primary "New instance" with the plus icon,
`aria-expanded` reflecting the form.

Content, top to bottom:

- New instance form (when open), a `<form>` card `aria-labelledby` its title:
  - Head: h2 "New instance", muted small "A plain game setup you manage yourself."
  - Body (`padding:14px`, wrapping row, gap 12px, aligned to the bottom):
    - "Name" (flex `2 1 200px`): text input, placeholder "My new world".
    - "Minecraft version" (flex `1 1 140px`): select from `minecraft_versions` (newest first),
      loaded when the form opens. The board's options are samples (26.3, 1.21.1, 1.20.1,
      1.16.5).
    - "Mod loader" (flex `1 1 140px`): select "None", "Fabric", "NeoForge", "Forge".
    - "Loader version" (flex `1 1 160px`): select with the one option "Newest for this version",
      disabled when the loader is None. The design offers no other option, so the core gets the
      loader without a version and stores the newest.
    - Buttons: secondary "Cancel" (closes and clears the name), primary submit "Create",
      disabled while the name is blank.
  - Foot: muted small "Minecraft 1.12.2 and older isn’t supported yet." (`padding:0 16px 12px`).
  - Create calls `instance_create(name, minecraft, loader)` (loader lower-case, none for None),
    closes the form and adds the row. While `minecraft_versions` is loading the version select
    shows one disabled option "Loading…"; when it fails (no internet) the form shows the error in
    an err note and Create stays disabled (the design does not show either; smallest honouring).
- Loading: one card of three skeleton rows (32 by 32, flexible 14px bar, 120 by 14, 140 by 30).
- Empty: empty state, h2 "No instances yet", text "Joining a server makes one for you
  automatically. You can also create a plain one with the button above.", primary button with
  the server icon "Go to Servers".
- Filled: one card.
  - Head row of column titles, 12px weight 700: "Name" (flexible), "Version" (190px), "Comes
    from" (230px), an empty 150px.
  - One row per instance (`instances`, sorted by name as the core returns them):
    1. Tile, or block cube 26px (static) in blocky.
    2. Flexible: the name, 15px weight 700, a button that opens the instance (ellipsis,
       `title`); under it, when its game runs, the ok pill "Game running".
    3. 190px: "Minecraft {minecraft}" weight 600; under it 12px muted the loader "{Loader}
       {version}" or "No mod loader".
    4. 230px: the kind badge, "Server", "Pack" or "By hand"; under it 12px muted ellipsis the
       source: server "{server} on {panel name}" plus " · {Catalogue} pack {version}" when it has
       a pack ("Homestead on Joppe’s Spawnpoint · Modrinth pack 1.3.7"); pack "{Catalogue} pack"
       ("FTB pack", "Modrinth pack"); by hand "Made by you".
    5. 150px, right-aligned, gap 6px: small secondary "Open" (`aria-label` "Open {name}"), small
       primary with the play icon "Play" (`aria-label` "Play {name}"); while its game runs the
       label is "Running", disabled, `aria-label` "{name} is running". Play calls
       `instance_launch(id)` through `withProgress`; disabled while another operation runs.
  - One row per instance folder the core could not read: a warning icon 20px in destructive in a
    34px slot; the folder in mono bold ("instances\old-test"), under it muted small "Portal can’t
    read this instance’s settings, so it can’t play it. The files are still there."; a small
    secondary button with the folder icon "Show folder". Needs core work (Beyond the core).

Sample (board): Survival (26.3, Fabric 0.20.1, Server, "Survival on Joppe’s Spawnpoint"),
Homestead (1.20.1, Fabric 0.18.4, Server, "Homestead on Joppe’s Spawnpoint · Modrinth pack 1.3.7",
running), Main (26.3, No mod loader, Server), Direwolf20 1.21 (1.21.1, NeoForge 21.1.77, Pack, "FTB
pack"), Fabulously Optimized (1.21.1, Fabric 0.16.5, Pack, "Modrinth pack"), Old times (1.16.5,
Forge 36.2.39, By hand, "Made by you"), and the unreadable `instances\old-test`. A newly created one
shows "Made by you, just now" in the board; use "Made by you".

Blocky differences: block cubes (static, 26px) instead of tiles; the unreadable icon `#8a1f1b`.

### One instance

Board: `PortalInstance.dc.html` (Homestead). No state picker: "Check again" switches the hand
files to done, Remove and Delete open their dialogs, Play marks it running, the filter and search
work.

Head (`align-items:center`, wraps):

- First line, full width: a 12px back button with the chevron icon, "Instances".
- Tile 44px with 18px initial (blocky: block cube 40px in a 52px slot, spinning and bobbing).
- Title block (flexible): h1 the instance name; summary "Minecraft {minecraft} · {Loader} {version} ·
  {Catalogue} pack {version} · from {server} on {panel}" with the parts that apply (sample:
  "Minecraft 1.20.1 · Fabric 0.18.4 · Modrinth pack 1.3.7 · from Homestead on Joppe’s
  Spawnpoint").
- Actions (gap 8px, wrap): label "Memory" (summary style) and a 92px select "4 GB", "6 GB" (the
  sample's choice), "8 GB", "10 GB" (Beyond the core); secondary "Mods folder" with the folder icon
  (`open_mods_folder`); danger outline "Delete" with the trash icon (opens the delete dialog);
  primary "Play" with the play icon (`instance_launch` through `withProgress`; "Running" and
  disabled while its game runs).

Content, top to bottom:

1. Files to download by hand (when there are any): a warn note, a `<section>` labelled by its
   heading:
   - h2 (inherits the note colour) "1 file to download by hand" ("{n} files to download by hand"
     for more), then "Its author doesn’t allow launchers to download it. Get it from its page and
     put it in the mods folder. It disappears from this list once it’s there." (for several:
     "Their authors don’t allow…" is not designed; keep the singular text or ask).
   - One line per file: the file name in bold mono ("cozy-lanterns-2.4.1+1.20.1.jar") and a small
     secondary "Open its page" with the external-link icon (`open_page(page)`; hidden when the
     file has no page).
   - Then small secondary "Open mods folder" with the folder icon (`open_mods_folder`) and small
     secondary "Check again".
   - When "Check again" finds every file in place: an info note, `role="status"`, ok check icon,
     "All files are in place. Homestead is ready to play." ("{name} is ready to play.").
   - Where the list comes from and how "Check again" checks it: Beyond the core.
2. Skipped files (when the last sync skipped any): an info note with the info icon,
   `aria-label="Skipped file"`: "The last sync left one pack file out:
   **iris-mc1.20.1-1.7.2.jar**. Your own file with the same name is in its place, and Portal never
   touches your files. Remove yours if you want the pack’s version." (file in bold mono). The
   plural is not designed. Needs core work.
3. Mods card, a `<section>` labelled "Mods":
   - Head: h2 "Mods", muted small "{n} mods" ("216 mods"), spacer, the segmented filter "All
     {n}", "Pack {n}", "Own {n}" ("All 216", "Pack 214", "Own 2"), a search input 170px by 30px
     with placeholder "Search mods" and an `sr-only` label "Search mods" (matches anywhere in the
     file name, ignoring case), a small secondary "Add a mod file" with the upload icon (the
     dialog plugin's `open` filtered to `.jar`, then `instance_add_mod(id, path)`, then reload).
   - First row, 12px, gap 8px: a 14px lock icon and "Pack mods are managed by the sync and can
     change with the next pack update. Your own mods are never changed or removed by a sync."
   - One row per mod (`instance_mods`; own first, then pack, each by name), `padding:7px 16px;
     min-height:34px`: file name mono, flexible, ellipsis, `title`; size 12px muted, 64px,
     right-aligned, tabular ("2.1 MB", one decimal, MB); a 70px slot with the badge "Own" or
     "Pack"; a 96px right-aligned slot: for own mods a small danger outline "Remove"
     (`aria-label` "Remove {file}", opens the remove dialog), for pack mods a muted 12px lock icon
     and "Managed". Pack = the core's `managed: true`.
   - The board lists 14 of 214 pack mods and ends with a centred muted row "+ 200 more pack mods.
     Scroll or search to find one." I read that as a shortcut of the sample, not a paging design:
     render every row and leave that line out (see Open questions).
   - Search with no match: one centred muted row "No mod matches “{q}”."
   - While `instance_mods` loads: the panel's `ModSkeleton` inside the card.
4. Remove dialog: title "Remove this mod?", body "This deletes **{file}** from {instance}’s mods
   folder. It’s your own file, so no sync will bring it back.", buttons "Cancel", "Remove mod"
   (`instance_remove_mod(id, name)`, then reload the list).
5. Delete dialog: title "Delete {name}?", body "This removes the whole instance folder: {n} mods
   (including your {k} own), its configs, and its saved worlds. It can’t be undone. You can always
   join the {server} server again to get a fresh copy.", buttons "Cancel", "Delete instance"
   (`instance_delete(id)`, then back to Instances). The last sentence fits a server instance
   only; for a pack or hand-made instance leave it out.

Status bar on this board: idle "Ready" + muted "Last played Homestead today, for 1 h 12 min."
(Beyond the core); playing "Playing Homestead" + muted "with {memory} of memory".

Sample: pack mods `fabric-api-0.92.2+1.20.1.jar` 2.1 MB, `create-fabric-0.5.1-f-build.1417+mc1.20.1.jar`
18.3 MB, `sodium-fabric-mc1.20.1-0.5.11.jar` 1.0 MB, `lithium-fabric-mc1.20.1-0.11.2.jar` 0.6 MB,
`FarmersDelight-1.20.1-2.1.1+refabricated.jar` 4.9 MB, `supplementaries-1.20-2.8.17-fabric.jar` 7.2
MB, `moonlight-1.20-2.13.31-fabric.jar` 2.8 MB, `jei-1.20.1-fabric-15.3.0.4.jar` 1.3 MB,
`waystones-fabric-1.20-14.1.3.jar` 0.7 MB, `balm-fabric-1.20.1-7.3.9.jar` 0.6 MB,
`cloth-config-11.1.118-fabric.jar` 1.0 MB, `modmenu-7.2.2.jar` 0.7 MB, `ferritecore-6.0.1-fabric.jar`
0.1 MB, `appleskin-fabric-mc1.20.1-2.5.1.jar` 0.1 MB; own `journeymap-1.20.1-5.9.18-fabric.jar` 4.6
MB, `iris-mc1.20.1-1.7.2.jar` 2.3 MB; 214 pack mods in all.

Blocky differences: the 40px spinning, bobbing sand cube instead of the 44px tile; the rest is the
token swap.

### Panels

Board: `PortalPanels.dc.html`. No state picker; the link card cycles through its phases by clicks
and a timer (it opens on the code phase; after 12 seconds it links; at 0 it expires).

Head: h1 "Panels"; summary "The Spawnpoint panels whose servers you can join. Portal keeps each
panel’s key in Windows Credential Manager and never shows it." No actions.

Content:

1. Linked panels card, a section labelled by its head: h2 "Linked panels", muted small "{n}
   panels" / "1 panel". One row per panel (`panels`):
   - The logo at 30px in a 34px slot.
   - Flexible: the name 15px weight 700 (`mc-name`, ellipsis); under it the address mono 12px
     muted ellipsis.
   - 300px status: answered: a 8px playing-green dot and "Connected · {n} servers"; failed: a
     16px warning icon in destructive and the reason ("The panel cannot be reached." in the
     board). Comes from `servers` (one call for the whole list); while it loads show nothing in
     this column.
   - Small danger outline "Remove" (`aria-label` "Remove {name}") opening the remove dialog.
   - No panels: not designed. Smallest: leave the card out when `panels` is empty.
2. Link a panel card, h2 "Link a panel", one of four phases:
   - Form (a `<form>`, `max-width:560px`, column gap 10px):
     - "Panel address": mono input, full width, placeholder "https://panel.example.com",
       `autocomplete="off"`; under it muted small "The address you open your Spawnpoint panel
       at."
     - When the address starts with `http://`: a warn note (`padding:10px 12px`) with an 18px
       warning icon and a checkbox label: "This panel has no https. **Link it anyway:** I
       understand Portal’s key will travel unencrypted." (Plan 5's wording "This panel has no
       https; link it anyway" is replaced by this.)
     - Primary submit with the link icon "Get a code": disabled until the address looks like
       `http(s)://host.tld…` and, for `http://`, the box is ticked. Calls
       `panel_pair_start(address, allow_http)`; its error shows in an err note under the field
       (not designed; smallest honouring).
   - Code (a wrapping row, gap 28px):
     - Left (flex `1 1 320px`, column gap 10px): muted "Approve this code in **{address}**"
       (address bold mono); the code display; a line with the spinner and "Waiting for approval.
       Portal notices by itself." (`aria-live="polite"`); muted small tabular "The code expires
       in {m:ss}." counting down from `expires_at`.
     - Right (flex `1 1 280px`): an ordered list "Open the approval page and sign in to your
       panel." / "Check that the panel shows the same code." / "Approve it. This window updates
       on its own."; then primary "Open the approval page" with the external-link icon
       (`open_pairing_page`) and secondary "Cancel" (back to the form; stops polling).
     - Polls `panel_pair_poll` every `interval` seconds; stops on a panel, at `expires_at`, on
       Cancel, or when the view is left (plan 5).
   - Linked (`role="status"`, row gap 12px): a 24px check icon in the ok colour, then 16px bold
     "Linked: {name}" and muted "Its servers now show in Servers.", primary "Go to Servers" with
     the server icon, secondary "Link another" (back to the form with the address reset to
     "https://"). The new panel appears in the linked list.
   - Expired (`role="alert"`): a 22px warning icon in destructive, bold "The pairing code expired
     or was already used.", muted "Codes last a few minutes. Get a new one and approve it in your
     panel.", primary "Get a new code" (calls `panel_pair_start` again with the same address).
     Shown when the countdown reaches 0 or `panel_pair_poll` fails.
3. Remove dialog: title "Remove {name}?", body "Its servers disappear from Servers and Portal
   forgets its key. Instances you made from its servers stay, and you can link it again later.",
   buttons "Cancel", "Remove panel" (`panel_remove(id)`).

The board's approval link is `{address}/launcher/approve`; the core opens
`{address}/settings/api-tokens` (plan 5, `open_pairing_page`). Follow the core.

Sample: "Joppe’s Spawnpoint" (`https://panel.joppek.be`, "Connected · 5 servers"), "Lien’s panel"
(`https://panel.lienb.be`, "The panel cannot be reached."); linking `https://spawnpoint.woutd.be`,
code "7ABR 547C", 4:32 left, which becomes "Wout’s Spawnpoint", "Connected · 2 servers".

Blocky differences: warning icons `#8a1f1b`; code 52px Ubuntu Mono; the rest is the token swap.

### Account

Board: `PortalAccount.dc.html`. No state picker; typing drives the validation.

Head: h1 "Account"; summary "Who you are when you join a server." Content column is
`max-width:760px`.

1. Player name card, a `<form>`: head h2 "Player name" and the badge "Offline". Body column gap
   10px: a row with the field "Name other players see" (flex `1 1 260px`, `maxlength="16"`,
   `autocomplete="off"`, `aria-describedby` the help, `aria-invalid` when invalid) and a primary
   submit "Save"; under it the help line, 12px:
   - valid and unchanged or changed: "Letters, numbers and _ only, 3 to 16 characters. Takes
     effect the next time you start a game."
   - shorter than 3: "Use at least 3 characters." in destructive
   - other invalid: "Only letters, numbers and _ are allowed." in destructive
   - just saved: "Saved. Your next game uses this name." and the button reads "Saved".
   - Save is disabled while invalid or unchanged. Loads with `settings_get`; saves with
     `settings_set(player_name)`; the sidebar footer updates.
2. A warn note (`role="note"`): bold "Servers in online mode will refuse this name." then "Without a
   Microsoft sign-in, you can only join servers that allow offline players. If a server turns you
   away with “Failed to verify username”, ask its owner to turn online mode off."
3. Microsoft account card: head h2 "Microsoft account" and the badge "Coming later". Body (wrapping
   row, gap 20px): text block (flex `1 1 380px`): "Sign in with the Microsoft account that owns
   Minecraft to join any server under your real name." and muted "It will work like linking a
   panel: Portal shows a code in large type, you enter it on Microsoft’s page, and Portal notices
   when you’re done. One account at a time."; a secondary button "Sign in with Microsoft",
   disabled. No command until plan 6.

Status bar sample on this board: "Playing Homestead" + muted "since 14:06 · as JoppeK".

Blocky differences: invalid help colour `#8a1f1b`; the rest is the token swap.

### Progress (the status bar)

Present at the bottom of every view, full width, `role="status"`. States, left to right:

| State | Shows |
| --- | --- |
| Idle | idle dot; "Ready"; muted "Join a server to start playing." (One instance board: "Last played {name} today, for 1 h 12 min.", Beyond the core) |
| Preparing | spinner; bold "Preparing {name}"; muted "Step {i} of {n}: {stage}"; the progress bar (flex 1); tabular "{done} of {total} files" (thousands with a comma: "1,204 of 3,891 files") |
| Starting | spinner; bold "Starting Minecraft for {name}…"; muted "The game window opens in a moment. You can keep using Portal." |
| Playing | playing dot; bold "Playing {name}"; muted "since {HH:MM} · connected to {hostname}" (a launch without a server: "since {HH:MM}") |
| Failed | background per token table; warning icon; bold "Couldn’t prepare {name}."; the error text (flex 1, one line with ellipsis in the board; see section 6); small secondary "Close" |

- `withProgress` (plan 5, Task 4) drives it; its label is the instance or server name.
- Stage names, from the core's `progress.stage`: `java` "Java", `libraries` "Libraries", `assets`
  "Game assets", `loader libraries` "Mod loader libraries", `loader` "Mod loader", `mods` "Mods";
  any other: the stage string with its first letter upper-cased. The board also shows "Checking
  files" for a sync that finds everything in place (the core reports no such stage; Beyond the
  core).
- The board's stage lists: a first join of a pack server: Java (1), Libraries (62), Game assets
  (3,891), Mod loader libraries (18), Mod loader (1), Mods (366); a rejoin: Checking files (214).
- Failure sample: Skyblock test fails at step 3, "This loader has no build for this Minecraft
  version. Pick another loader for this server in its panel, then try again."
- While Preparing or Starting, every button that starts an operation (Join, Play) is disabled. Close returns to Idle, and the button works again.
- After Playing: when `game-exited` arrives for that instance, back to Idle. The design draws no
  exit message (see Beyond the core).
- The window stays open and usable while the game runs ("You can keep using Portal").

Blocky: bar `#2b2b2b` with a 4px `#111` top border; failed `#5a1c1a`; dots square; the XP-bar
progress.

## 5. Beyond the core

What the design shows that the core as built (plan 5's command table and "Carried over") cannot
supply, and the smallest way to honour it.

| Design shows | Missing | Smallest honouring |
| --- | --- | --- |
| "Step 6 of 6: Mods" | Progress carries `stage`, `done`, `total`; no step number or count of steps | Leave "Step i of n:" out and show the stage name; or core adds `step` and `steps` to `Progress` |
| "Checking files" stage on a rejoin | No progress is sent while a sync only hashes | Core sends a `check` stage while hashing (useful anyway: 366 files hash on every join); or show "Checking files" with an empty bar until the first event |
| Starting, then Playing | `join` resolves once the game process exists; nothing marks the gap | Treat the resolve as Playing; show Starting from the last progress event to the resolve. Or core sends a `starting` stage before spawning |
| "Playing … connected to {host}", "Game running", "Running" on Play | In-session only: after the window restarts it does not know a game is running | Keep it in-session (a `ref` set on resolve, cleared on `game-exited`); accept the restart gap |
| "Last played Homestead today, for 1 h 12 min." | No play time is recorded | Leave it out (show "Ready" and nothing after it); or core writes `last_played` and duration to `instance.json` on exit |
| Memory select per instance; "with 6 GB of memory" | `play::start_instance` takes `memory_mb`, but no per-instance value is stored and `instance_launch`/`join` take none | Core: `memory_mb: Option<u32>` in `instance.json` and an `instance_set_memory(id, mb)` command; `join` and `instance_launch` read it (default 4096). Or leave the select out for now |
| Panel failure in words ("The panel cannot be reached.", the per-error advice in the failed status) | Errors arrive as one string | Show the core's error text as it is (plan 5 already says so); words per error kind later |
| Unreadable instances row with "Show folder"; "1 couldn’t be read" | `instance::list` skips unreadable folders silently ("Carried over"); `open_mods_folder` builds its path through `instance::load`, which fails for them | Core: `list` also returns the folder names it could not read; command `open_instance_folder(id)` that builds the path from a checked id without loading it |
| Files to download by hand, on the instance's page, with "Check again" | `join` returns `Vec<Manual>` once; nothing keeps it, nothing re-checks it | Core keeps the manual files in `instance.json` at each sync and `instance_mods` (or a new `instance_manual(id)`) returns those not yet in `mods/`. Window-only fallback: keep `join`'s answer for the session and filter it by `instance_mods` names on "Check again" |
| "The last sync left one pack file out: …" | Skipped files are silent ("Carried over") | Core: the sync returns and stores the skipped paths; shown from `instance.json` |
| Pack version "1.3.7" in an instance's source | `PackRef.version` is the version id (for FTB a number like "100596"); the name is not kept | Core stores `version_name` in `PackRef`; until then show "{Catalogue} pack" without a version |
| Panel name and server name in "Homestead on Joppe’s Spawnpoint" | `Source::Server` keeps panel id and server slug | No core change: panel name from `panels`, server name = instance name (it is made from the server's name). A removed panel: show the id |
| Theme and appearance choice | Not in core or design | `localStorage`, window only (section 1) |
| Custom title bar | Window config, not core | `decorations: false`, drag region, four window permissions (section 1) |

Open decisions the owner asked about, and what the design says:

| Decision | The design |
| --- | --- |
| Where progress lives | Decided: a 44px status bar at the bottom of every view; one operation at a time; failures stay there until Close. |
| What the window shows after the game starts, and when it closes | Decided for after start: the window stays open and usable; the status bar says "Playing {name} since {HH:MM} · connected to {host}"; that server's Join reads "Playing" (disabled) and the instance shows "Game running" and a disabled "Running". When the game closes: not drawn; the One instance board's idle line ("Ready · Last played …") implies a return to Ready. A crash (non-zero exit code) is not addressed. |
| Skipped files | Decided: an info note on the instance's page naming the file, saying the player's own file is in its place and how to get the pack's version. Needs core work. |
| Disabling a pack mod | Not offered: pack mods show "Managed" with a lock and no control; the note says they "can change with the next pack update". The `.jar.disabled` question is not addressed. |
| Unreadable instances | Decided: listed in Instances with their folder, an explanation and "Show folder"; counted in the summary. Needs core work. |
| Installing a pack without a server | Not addressed: pack instances are listed (kind "Pack"), but there is no way to make one in the window. The command line keeps doing it (plan 5, Task 6). |
| Memory per instance | Decided: a select on the instance's page, 4, 6, 8 or 10 GB, sample 6 GB. Needs core work. |
| Starting without internet | Partly: Servers has "No panel answered." with Try again; Instances and the instance page need no network in the design. Not addressed: the New instance form's version list (network) and whether Play works offline (`java::ensure` touches the network; open in "Carried over"). |
| Account before Microsoft sign-in | Decided: the offline player name with its rule, a warning that online-mode servers refuse it, and a disabled "Sign in with Microsoft" marked "Coming later". |

## 6. Accessibility and behaviour

- Keyboard order follows the DOM: title bar buttons, sidebar items, the account footer link, the
  view's head actions, the content top to bottom, the status bar's Close. Nav items are buttons
  (Enter and Space). The segmented filter is three toggle buttons (Tab between them). Esc closes a
  dialog (Cancel); focus returns to the button that opened it. After a view switch, move focus to
  the view's h1 (`tabindex="-1"`) so a screen reader hears where it is.
- Focus: the outline in the token table on every focusable element (`:focus-visible`), never
  removed. Blocky: `3px solid #ffffff` is 1.71:1 against the `#c6c6c6` cards and dialogs (below
  3:1), so inside them use `#111111` (11.05:1). The panel has the same flaw.
- Icon-only controls: only the three title bar buttons, which have `aria-label`s. Buttons whose
  text repeats per row carry the row's name (`aria-label` "Join {name}", "Open {name}", "Play
  {name}", "Remove {file}", "Remove {panel}"). Every decorative icon is `aria-hidden="true"`. In
  the narrow sidebar (below) the items become icon-only and need `aria-label`s.
- Live regions: the status bar is `role="status" aria-live="polite"`, but the board updates its
  count every 110ms inside it, which floods a screen reader. Keep the counts and the bar outside
  the live text; announce only state changes ("Preparing Homestead", each new stage, "Playing
  Homestead", the failure). Failures are `role="alert"`.
- The failed status shows the error on one line with an ellipsis; a long error is cut off. Let the
  bar grow (`min-height:44px`, the text wrapping) so the whole error is readable.
- Contrast, computed from the tokens above (text needs 4.5:1, borders and marks 3:1):
  - Fails, text: blocky card head white `#ffffff` on `#8b8b8b` 3.41:1, its muted `#f0f0f0` on
    `#8b8b8b` 2.99:1. Darken the head to `#6e6e6e` (white 5.10:1).
  - Fails, component borders: standard input border, board `#c9ced3` 1.58:1 and panel `#dee2e6`
    1.30:1 on white; `#8a9097` (the panel's `--ring`) gives 3.22:1. Dark `#353a40` on `#171a1e`
    is 1.52:1.
  - Below 3:1 but paired with text, so acceptable: idle dot `#9aa2aa` on white 2.59:1; progress
    fill `#3fa36b` on its track 2.52:1.
  - Just passes: white on `#3c8527` (blocky primary, active nav, Running pill) 4.59:1; the focus
    green `#3fa36b` on white 3.15:1.
  - Everything else listed passes 4.5:1 (light muted on white 5.82:1, on `#f1f2f3` 5.19:1, on
    the failed bar 4.77:1; all pills, badges and notes 5.14:1 or more; dark muted on `#171a1e`
    6.75:1; blocky muted in cards 6.36:1, outside 8.99:1 or more).
- Minimum size 900 by 600. At 1100 the content column is 840px wide (1100 − 212 − 48). At 900 it
  would be 640px, and the fixed columns no longer fit: Servers needs 568px besides the name,
  Instances 692px (columns, 14px gaps and 16px row padding). So below a window width of 1000px the sidebar narrows to 64px: logo only, the
  four items icon-only (`aria-label` and `title` with the label), the footer avatar only; and the
  Instances "Comes from" column narrows from 230px to 160px. That leaves the name about 218px
  (Servers) and 164px (Instances); names ellipsize. The One instance head's actions wrap to a
  second line (it already wraps). Height 600: title bar, head and status bar keep about 150px;
  the content scrolls in the remaining 450px; dialogs (about 200px high) fit.
- Long names: server, instance, panel and file names and addresses are one line with an ellipsis
  and the full text in `title`; the One instance h1 likewise.
- Long lists: the content area scrolls, never the sidebar, head or status bar (plan 5's review
  focus). A pack of 300 mods renders 300 rows of 34px; no virtual list needed at that size.
- Reduced motion: stop all animation in both themes (spinner may keep turning only if it is the
  only sign of work; the text says it too).

## 7. Open questions for the owner

1. Where should the theme (standard or blocky) and, for standard, light / dark / system be
   chosen? The design has no control. Proposal: a "Look" card at the bottom of Account.
2. The panel's blocky pages put a sunset sky behind each page heading (`BlockySky.vue`); the
   blocky Portal boards have none. Leave it out (the boards), or use the panel's compact sky behind
   the head of each view?
3. The status bar's trailing text differs per board ("connected to {host}", "with 6 GB of
   memory", "as JoppeK"). One line everywhere ("since {HH:MM} · connected to {host}"), or does it
   change with the view on purpose?
4. "+ 200 more pack mods. Scroll or search to find one.": a sample shortcut (render every mod, as
   assumed here), or should the list show a first part and the rest on demand?
5. Memory per instance, play time ("Last played … for 1 h 12 min"), skipped files, kept
   hand-fetch files and unreadable instances all need core changes (section 5). Build them in plan
   5, or ship the window without those elements first?
6. The blocky card heads fail contrast (white on `#8b8b8b`, 3.41:1). Darken them to `#6e6e6e`,
   or switch their text to `#1b1b1b`?

## Decisions by the owner (20 November 2026)

These settle the open questions above and override anything earlier in this document that says otherwise.

1. **Everything in the design is built.** The core gains what section 5 lists as missing: memory per instance (4, 6, 8 or 10 GB), "Last played", a pack's version name, the note for files a sync skipped, unreadable instances with "Show folder", hand-fetch files with "Check again", and the finer progress ("Step i of n", "Checking files", "Starting"). Nothing in the window is a placeholder.
2. **Theme choice lives in an "Appearance" card on the Account view**: standard or blocky, and for standard light, dark or follow Windows. Stored on this PC only.
3. **The blocky theme is corrected for contrast**: card heads `#6e6e6e`, and the focus ring gets a dark outline so it is visible on grey. Text meets 4.5:1.
4. **The mod list is one scrolling list** inside the view, own mods first, with a search field. The "+ 200 more pack mods" line in the board is a mock-up shortcut and is not built.

Not asked, decided by default: the panel's blocky sky is not put behind headings; the status bar's trailing text follows the boards per view.
