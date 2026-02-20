# Chopin Documentation

Comprehensive guides for the Chopin web framework.

## Getting Started

- [**QUICK_START.md**](QUICK_START.md) — Installation, your first app (5 min), authentication examples
- [**FEATURES.md**](FEATURES.md) — Complete feature matrix, security options, what's included

## Architecture & Design

- [**ARCHITECTURE.md**](../ARCHITECTURE.md) — Complete system design, component architecture, and design principles
- [**modular-architecture.md**](modular-architecture.md) — ChopinModule trait, MVSR pattern, hub-and-spoke design, module development guide

## Performance

- [**PERFORMANCE_OPTIMIZATION.md**](PERFORMANCE_OPTIMIZATION.md) — Complete performance tuning guide (HTTP layer, **FastRoute**, headers, JSON, allocators)
- [**BENCHMARKS.md**](BENCHMARKS.md) — Performance comparisons with 7 frameworks, cost analysis, optimization tips
- [**json-performance.md**](json-performance.md) — SIMD JSON optimization with sonic-rs, thread-local buffering

> 💡 **FastRoute quick reference:** `FastRoute::text("/plaintext", b"Hello")` (~35ns static), `FastRoute::json_serialize("/json", || Msg{..})` (~100-150ns dynamic). See [PERFORMANCE_OPTIMIZATION.md § FastRoute](PERFORMANCE_OPTIMIZATION.md) and the [Advanced Tutorial](tutorial-advanced.html).

## Debugging

- [**Debugging & Logging**](debugging-and-logging.md) — Complete guide to enabling request logs, error traces, and debugging
- [**LOGGING.md**](LOGGING.md) — Quick reference for logging configuration

## Tutorials (HTML)

- [**Tutorial Index**](tutorial-index.html) — Interactive tutorial guide with organized topics

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
├── QUICK_START.md               # Get started in 5 minutes
├── FEATURES.md                  # Feature matrix & capabilities
├── BENCHMARKS.md                # Performance comparisons
├── PERFORMANCE_OPTIMIZATION.md  # Complete performance tuning guide
├── ARCHITECTURE.md              # System design (in repo root)
├── modular-architecture.md      # Module development guide
├── debugging-and-logging.md     # Debugging guide
├── LOGGING.md                   # Logging quick reference
├── json-performance.md          # JSON optimization guide
├── index.html                   # Landing page
├── tutorial-index.html          # Tutorial guide index
├── tutorial-basics.html         # Tutorial: Getting started
├── tutorial-database.html       # Tutorial: Database & models
├── tutorial-api.html            # Tutorial: Building APIs
├── tutorial-modules.html        # Tutorial: Modular architecture
├── tutorial-advanced.html       # Tutorial: Advanced features
├── tutorial-deployment.html     # Tutorial: Testing & deployment
├── css/
│   ├── style.css                # Main styles
│   └── tutorial.css             # Tutorial styles
├── js/
│   └── main.js                  # JavaScript interactions
└── img/
    └── ...                       # Images and diagrams
```

## Tutorial Structure

The tutorial is organized into focused, topic-specific pages:

- **tutorial-index.html** — Landing page with guide overview
- **tutorial-basics.html** — Installation, Hello World, Debugging
- **tutorial-database.html** — Configuration, Models, Migrations
- **tutorial-api.html** — Modules, Routing, Authentication, Security
- **tutorial-modules.html** — MVSR Pattern, ChopinModule, Composition
- **tutorial-advanced.html** — OpenAPI, Caching, File Uploads, GraphQL
- **tutorial-deployment.html** — Testing, Performance, Production Deployment

Each guide is self-contained and can be read independently or in sequence.

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
