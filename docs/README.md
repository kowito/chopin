# Chopin Website

This is the GitHub Pages website for the Chopin web framework.

## Local Development

To preview the site locally:

```bash
# Using Python
cd website
python3 -m http.server 8000

# Using Node.js
npx http-server website -p 8000

# Then open http://localhost:8000
```

## Deployment

This site is deployed via GitHub Pages. To enable:

1. Go to your repository settings
2. Navigate to **Pages** section
3. Under **Source**, select:
   - **Branch**: `main`
   - **Folder**: `/website`
4. Click **Save**

Your site will be available at: `https://kowito.github.io/chopin/`

## Structure

```
website/
├── index.html          # Main landing page
├── css/
│   └── style.css       # Styles
├── js/
│   └── main.js         # JavaScript interactions
├── debugging-and-logging.md  # Complete debugging guide
├── LOGGING.md          # Quick logging reference
├── json-performance.md # JSON performance guide
└── README.md           # This file
```

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
