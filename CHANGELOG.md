# Changelog

All notable changes to Wallflower will be documented in this file.

## [0.2.0] - 2025-01-25

### Added
- Native trackpad swipe and mouse button navigation (back/forward)
- Author links in article headers (clickable to view author profile)
- Hero header images for articles with parallax-style display
- Loading overlay for smooth transitions between articles
- Responsive action buttons that collapse to menu on narrow screens
- Liquid glass effect for controls over hero images

### Changed
- Redesigned nav bar with tabs and centered search field
- Moved markdown export actions to nav bar with circular buttons
- Extended cache duration to 48 hours
- Articles now fall back to cached version on fetch failure
- Input clears automatically when focused while viewing an article
- Input blurs after article loads for cleaner UX

### Fixed
- Page flash during article transitions
- Input staying focused after article loads

## [0.1.0] - 2025-01-24

### Added
- Initial release
- Fetch and read Medium articles via Freedium endpoints
- Recent articles history
- Favorites management
- Light, dark, and sepia themes
- Adjustable font size
- Copy article as Markdown
- Save article as Markdown file
- Keyboard shortcuts (Cmd+L, Cmd+R, Cmd+D, Cmd+S, etc.)
