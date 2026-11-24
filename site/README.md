# Eollo product site

Static product landing page for Eollo: positioning, capability catalog, install steps, MCP client
registration, skills and the security model. Built with React + Vite and deployed to GitHub Pages.

This is separate from `ui/` (the internal routing-graph viewer). The site has no backend and never
calls the Eollo runtime; every number and capability card is derived from the repository.

## Develop

```bash
cd site
npm install
npm run dev        # http://localhost:5174
```

Two entries exist:

- `index.html` — 中文 (`lang="zh-CN"`)
- `en.html` — English (`lang="en"`)

Each entry mounts the same `src/App.jsx` with a `lang` prop; copy lives in `src/i18n.js`.

## Build

```bash
npm run build      # writes site/dist and copies schemas/ into dist/schemas
npm run preview
```

The `postbuild` step copies `schemas/` into `dist/schemas`, so the deployed site serves
`https://zhangpeibin.github.io/eollo/schemas/...`, matching the `$id` values in `schemas/`.

`base: "./"` keeps every asset link relative, so the same build works locally, under the GitHub
Pages project path (`/eollo/`), and on a custom domain once one is configured. Do not add
root-absolute URLs (`/assets/...`).

## Capability data

`src/data/capabilities.json` is generated from the repository catalog and checked in. It carries
identity, status, chains, execution, `requires` prerequisites and workflow step lists. Regenerate it
after editing `capabilities/catalog.yaml`:

```bash
cargo run -q -p eollo-protocol --bin catalog-json > site/src/data/capabilities.json
```

`cargo test -p eollo-protocol site_catalog_matches_catalog` fails when the file is stale. The site
renders cards from this file, so `capabilities/catalog.yaml` stays the single source of truth.

Chinese summaries are display copy in `src/i18n.js`; English summaries come straight from the
catalog. Add a translation when adding a capability.

## Managed remote endpoint

`src/config.js` exports `MANAGED_REMOTE_URL`, read from `VITE_MANAGED_REMOTE_URL` at build time. Leave
it empty until the hosted service is live: the Managed endpoint tab then shows a "soon" state and
self-host is the default. Set it at build time and the tab renders
`EOLLO_REMOTE_URL=<url> eollo mcp` automatically.

For GitHub Pages, add a repository **variable** (Settings → Secrets and variables → Actions →
Variables):

- name: `MANAGED_REMOTE_URL`
- value: `https://<hosted-endpoint>`

`pages.yml` passes it to `npm run build`; re-run the workflow.

## Deploy

`.github/workflows/pages.yml` builds `site/dist` and deploys it on pushes to `master` that touch
`site/`.

One-time repository setup:

1. Open **Settings → Pages**.
2. Set **Source** to **GitHub Actions**.
3. Run the workflow (push to `master` or **Actions → Pages → Run workflow**).

Expected URLs:

- Chinese: <https://zhangpeibin.github.io/eollo/>
- English: <https://zhangpeibin.github.io/eollo/en.html>

No custom domain is configured yet. The site is served from the GitHub Pages project path. When a
product domain is purchased, add `site/public/CNAME`, point DNS at GitHub Pages, and update the
absolute `hreflang` links in `index.html` and `en.html` plus the schema `$id` values.

## Structure

```text
site/
  index.html            # 中文 entry
  en.html               # English entry
  src/main.jsx          # mounts App with the entry language
  src/App.jsx           # section composition
  src/config.js         # MANAGED_REMOTE_URL switch for the hosted endpoint
  src/i18n.js           # all zh/en copy, snippets and links
  src/styles.css        # dark theme, responsive layout, motion
  src/data/capabilities.json
  src/components/
    Nav.jsx             # sticky nav, mobile drawer, language switch
    Hero.jsx            # full-bleed layer stack: map behind copy, scrim, scroll cue
    CapabilityMap.jsx   # SVG capability constellation: tour, hover, requires mode, detail card
    RouteField.jsx      # canvas routing graph, reused as the flow route preview
    Problems.jsx
    Flow.jsx            # scroll-driven progress through the swap workflow
    Capabilities.jsx    # filterable catalog cards with spotlight hover
    Install.jsx         # steps + MCP client tabs + remote service card
    RemoteCard.jsx      # managed endpoint / self-host tabs
    Skills.jsx          # skills grid + rotating prompt copier
    Safety.jsx
    Footer.jsx
    Reveal.jsx          # IntersectionObserver reveal wrapper
    CopyButton.jsx
    CodeBlock.jsx
    CursorGlow.jsx
```

## Motion and accessibility

- Motion is CSS, SVG and canvas only; no animation dependency.
- The hero map auto-tours one workflow at a time; hovering a node, the toolbar or the detail card
  pauses it. It also pauses offscreen, when the document is hidden and under reduced motion.
- The hero map is the first section's full-width background: 7 domain hubs ring the whole section,
  the copy is centered above a radial scrim, and hub labels inside the text zone are hidden so the
  graph stays a background instead of competing with the headline.
- `prefers-reduced-motion` disables the aurora, cursor glow, packet animation and reveal offsets.
- The canvas pauses when offscreen and on reduced-motion; touch devices skip the cursor glow.
- Anchor targets use `scroll-padding-top` so the fixed nav does not cover headings.
- Interactive controls are real `button`/`a` elements with visible focus rings.

## Content freshness

Sources: `README.md`, `README.zh.md`, `docs/MCP.md`, `skills/*/SKILL.md` and
`capabilities/catalog.yaml`. When those change, update `src/i18n.js` and regenerate the catalog JSON.
