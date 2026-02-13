# Codirigent Website

Landing page for codirigent.dev

## Local Development

1. Install dependencies:
   ```bash
   gem install bundler
   bundle install
   ```

2. Run Jekyll locally:
   ```bash
   bundle exec jekyll serve
   ```

3. Open http://localhost:4000

## Deployment

Automatically deployed to GitHub Pages on push to main branch.

## Structure

- `_config.yml` - Jekyll configuration
- `_layouts/` - HTML templates
- `_includes/` - Reusable components
- `assets/` - CSS, JS, images
- `index.md` - Homepage content

## Automation

- Version and download links auto-update on release via `.github/workflows/update-website.yml`
- GitHub Pages builds automatically from `/website` folder
