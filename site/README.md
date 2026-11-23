# Fluxion product site

Static product landing page for Fluxion: positioning, capability catalog, install steps, MCP client
registration, skills and the security model. Built with React + Vite and deployed to GitHub Pages.

This is separate from `ui/` (the internal routing-graph viewer). The site has no backend and never
calls the Fluxion runtime; every number and capability card is derived from the repository.

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
npm run build      # writes site/dist
npm run preview
```

`base: "./"` keeps every asset link relative, so the same build works locally and under the GitHub
Pages project path (`/Fluxion/`). Do not add root-absolute URLs (`/assets/...`).

## Capability data

`src/data/capabilities.json` is generated from the repository catalog and checked in. Regenerate it
after editing `capabilities/catalog.yaml`:

```bash
cargo run -q -p fluxion-protocol --bin catalog-json > site/src/data/capabilities.json
```

`cargo test -p fluxion-protocol site_catalog_matches_catalog` fails when the file is stale. The site
renders cards from this file, so `capabilities/catalog.yaml` stays the single source of truth.

Chinese summaries are display copy in `src/i18n.js`; English summaries come straight from the
catalog. Add a translation when adding a capability.

## Deploy

`.github/workflows/pages.yml` builds `site/dist` and deploys it on pushes to `master` that touch
`site/`.

One-time repository setup:

1. Open **Settings → Pages**.
2. Set **Source** to **GitHub Actions**.
3. Run the workflow (push to `master` or **Actions → Pages → Run workflow**).

Expected URLs:

- Chinese: <https://zhangpeibin.github.io/Fluxion/>
- English: <https://zhangpeibin.github.io/Fluxion/en.html>

If you add a custom domain, update the absolute `hreflang` links in `index.html` and `en.html`.

## Structure

```text
site/
  index.html            # 中文 entry
  en.html               # English entry
  src/main.jsx          # mounts App with the entry language
  src/App.jsx           # section composition
  src/i18n.js           # all zh/en copy, snippets and links
  src/styles.css        # dark theme, responsive layout, motion
  src/data/capabilities.json
  src/components/
    Nav.jsx             # sticky nav, mobile drawer, language switch
    Hero.jsx            # headline, install command, stats
    RouteField.jsx      # canvas routing graph (pointer interaction)
    Problems.jsx
    Flow.jsx            # scroll-driven progress through the swap workflow
    Capabilities.jsx    # filterable catalog cards with spotlight hover
    Install.jsx         # steps + MCP client tabs
    Skills.jsx          # skills grid + rotating prompt copier
    Safety.jsx
    Footer.jsx
    Reveal.jsx          # IntersectionObserver reveal wrapper
    CopyButton.jsx
    CodeBlock.jsx
    CursorGlow.jsx
```

## Motion and accessibility

- Motion is CSS and canvas only; no animation dependency.
- `prefers-reduced-motion` disables the aurora, cursor glow, packet animation and reveal offsets.
- The canvas pauses when offscreen and on reduced-motion; touch devices skip the cursor glow.
- Anchor targets use `scroll-padding-top` so the fixed nav does not cover headings.
- Interactive controls are real `button`/`a` elements with visible focus rings.

## Content freshness

Sources: `README.md`, `README.en.md`, `docs/MCP.md`, `skills/*/SKILL.md` and
`capabilities/catalog.yaml`. When those change, update `src/i18n.js` and regenerate the catalog JSON.
