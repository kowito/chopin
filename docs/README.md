# Chopin Documentation

Comprehensive guides for the Chopin web framework.

## Architecture & Design

- [**ARCHITECTURE.md**](../ARCHITECTURE.md) — Complete system design, component architecture, and design principles
- [**Modular Architecture**](modular-architecture.md) — ChopinModule trait, MVSR pattern, hub-and-spoke design

## Performance

- [**JSON Performance**](json-performance.md) — SIMD JSON optimization with sonic-rs

## Debugging

- [**Debugging & Logging**](debugging-and-logging.md) — Complete guide to enabling request logs, error traces, and debugging
- [**LOGGING.md**](LOGGING.md) — Quick reference for logging configuration

## Tutorials (HTML)

- [**Tutorial Index**](tutorial-index.html) — Interactive tutorial guide (recommended starting point)
- [**Complete Tutorial**](tutorial.html) — All content in one page

## GitHub Pages Website

This directory is deployed as a GitHub Pages site. To preview locally:

```bash
# Using Python
cd docs
python3 -m http.server 8000

# Using Node.js
npx http-server docs -p 8000

# Then open http://localhost:8000
```

## Deployment

This site is deployed via GitHub Pages:

1. Go to repository settings → **Pages**
2. Set **Source** to Branch: `main`, Folder: `/docs`
3. Click **Save**

Site URL: `https://kowito.github.io/chopin/`

## Directory Structure

```
docs/
├── README.md                     # This file
├── ARCHITECTURE.md               # System design (in repo root)
├── modular-architecture.md       # Module development guide
├── debugging-and-logging.md      # Debugging guide
├── LOGGING.md                    # Logging quick reference
├── json-performance.md           # JSON optimization guide
├── index.html                    # Landing page
├── tutorial-index.html           # Tutorial guide index
├── tutorial.html                 # Complete tutorial
├── css/
│   ├── style.css                 # Main styles
│   └── tutorial.css              # Tutorial styles
├── js/
│   └── main.js                   # JavaScript interactions
└── img/
    └── ...                       # Images and diagrams
```

## Tutorial Structure

The tutorial is available in two formats:

1. **Organized Navigation** - `tutorial-index.html` 
   - Friendly landing page with organized topics
   - Quick access to specific sections
   - Recommended for new users
   - Better mobile experience

2. **Complete Reference** - `tutorial.html`
   - All content in one comprehensive page
   - Full table of contents sidebar
   - Easy to search and reference
   - Better for advanced users who want everything at once

## Features

- **Responsive Design** — Mobile-friendly layout
- **Performance Benchmarks** — Interactive charts
- **Code Examples** — Syntax-highlighted examples with copy buttons
- **Modern UI** — Clean, professional design
- **Zero Dependencies** — Pure HTML/CSS/JS, no frameworks
- **Fast Loading** — Optimized for performance

## Customization

### Colors

Edit CSS variables in `css/style.css`:

```css
:root {
    --color-primary: #FF6B35;
    --color-secondary: #004E89;
    --color-accent: #1A936F;
    /* ... */
}
```

### Content

Edit `index.html` to update:
- Benchmark numbers
- Code examples
- Feature descriptions
- Links and CTAs

## License

WTFPL (Do What The Fuck You Want To Public License)
