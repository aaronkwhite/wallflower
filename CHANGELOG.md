# Changelog

All notable changes to Wallflower will be documented in this file.

## [0.3.7] - 2026-06-03

### Fixed
- Code-block copy buttons now work: they copy the snippet to the clipboard (previously they relied on Freedium's stripped JavaScript and did nothing)
- Copy buttons are now positioned in the top-right corner of each code block instead of floating above it, and show a checkmark on success

## [0.3.6] - 2026-06-03

### Fixed
- Article fetching works again after Freedium changed its `__data.json` format — the parser now reads the page payload from the SvelteKit `nodes`/`eager` structure (with backward compatibility for the previous chunk format)
- Removed the defunct `freedium.cfd` endpoint (no longer resolves); `freedium-mirror.cfd` is now the sole default, so failures report the real error instead of a misleading DNS error

## [0.3.5] - 2026-05-31

### Fixed
- Article fetching now works again after Freedium migrated to a SvelteKit SSR app — the parser reads the devalue-encoded `__data.json` payload instead of scraping HTML
- Inline article images and header images now load correctly by resolving host-relative URLs against the Freedium endpoint
- Empty server-side renders now surface an error instead of showing a blank article

## [0.3.4] - 2026-02-22

### Added
- YouTube video thumbnails with play button overlay — click to open in browser
- Vimeo embed detection with external playback link
- Cmd+K search now works from article view (navigates back to start page with search open)

### Fixed
- Search overlay now dismisses when opening an article from search results
- Escape key now works to dismiss the error screen and return to start page
- Search button click from article view now works correctly

## [0.3.3] - 2026-02-21

### Fixed
- Search overlay now dismisses when opening an article from search results
- Escape key now works to dismiss the error screen and return to start page

## [0.3.2] - 2026-02-21

### Added
- Infinite scroll for history list — loads more articles as you scroll down
- Database import from backup files via Settings

## [0.3.1] - 2025-01-27

### Added
- Confetti celebration when adding articles to favorites

## [0.3.0] - 2025-01-26

### Added
- Full-text search with SQLite FTS5 across article titles, authors, and content
- Search overlay with Cmd+K shortcut and live results as you type
- Sort articles by Recent, Title, Author, or Date Added
- Animated splash screen with rotating logo and version badge
- Arrow key navigation for article list (up/down to select, Enter to open)
- Escape key hierarchy: close modal → blur input → clear selection → go home

### Changed
- Prioritized working Freedium mirror endpoint
- Article list items now have rounded corners (28px)
- Improved error handling for search queries with special characters

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
