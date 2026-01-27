<p align="center">
  <img src="icons/logos/wallflower-logo.svg" alt="Wallflower" width="200">
</p>

# Wallflower

![Version](https://img.shields.io/badge/version-0.3.1-blue)
![License](https://img.shields.io/badge/license-MIT-green)

A quiet reader for consuming articles. Cross-platform desktop application for macOS, Windows, and Linux.

Wallflower is a lightweight client wrapper for [Freedium](https://freedium.cfd), providing a distraction-free reading experience with local caching, favorites, and export capabilities.

## Features

- **Clean Reading Experience** - Distraction-free article viewing with customizable themes
- **Full-Text Search** - Search across article titles, authors, and content with SQLite FTS5
- **Local Caching** - Articles cached locally in SQLite for offline reading
- **Favorites & History** - Track what you've read and save articles for later
- **Export Options** - Save articles as Markdown files or copy to clipboard
- **Cross-Platform** - Native builds for macOS, Windows, and Linux
- **Multiple Themes** - System, Light, Dark, and Sepia modes
- **Keyboard Navigation** - Full keyboard support with arrow keys and shortcuts
- **Customizable** - Adjustable font size and content width

## Tech Stack

- **Backend**: Rust + Tauri 2
- **Frontend**: Vanilla JS + CSS
- **Database**: SQLite (bundled)
- **Parsing**: Scraper (CSS selectors)

## Installation

Download the latest `.dmg` from [Releases](https://github.com/aaronkwhite/wallflower/releases):

- **Apple Silicon (M1/M2/M3)**: `Wallflower_x.x.x_aarch64.dmg`
- **Intel**: `Wallflower_x.x.x_x64.dmg`

Open the DMG and drag Wallflower to your Applications folder.

### Building from Source

```bash
# Prerequisites: Rust toolchain and Tauri CLI
cargo install tauri-cli

# Build
cargo tauri build
```

## Usage

1. Paste an article URL into the search bar
2. Press Enter or click the fetch button
3. Read in a clean, focused interface

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Cmd+K` | Open search |
| `Cmd+L` | Focus URL input |
| `↑` / `↓` | Navigate article list |
| `Enter` | Open selected article |
| `Escape` | Close/blur/clear selection |
| `Cmd+R` | Refresh article |
| `Cmd+D` | Toggle favorite |
| `Cmd+S` | Save as Markdown |
| `Cmd+Shift+C` | Copy as Markdown |

## Configuration

Settings are stored in `~/.config/Wallflower/config.toml`:

```toml
endpoints = [
  "https://freedium.cfd/",
  "https://freedium-mirror.cfd/"
]
theme = "system"
font_size = 17
max_width = 680
```

## Similar Projects

- [free-medium](https://github.com/fferrin/free-medium) - Browser extension wrapper for Freedium

## Disclaimer

**This software is provided for educational and experimental purposes only.**

Wallflower is a client application that interfaces with the third-party [Freedium](https://freedium.cfd) service. The developers of Wallflower:

- Do not operate, maintain, or control the Freedium service
- Make no guarantees about the availability or functionality of Freedium
- Are not responsible for how users choose to use this software
- Do not encourage or condone any violation of terms of service of any platform

**Use at your own risk.** Users are solely responsible for ensuring their use of this software complies with all applicable laws and terms of service. The authors and contributors disclaim all liability for any misuse of this software.

This project is an independent experiment and is not affiliated with, endorsed by, or connected to any content platform or the Freedium project.

## Support

If you find this useful, consider buying me a coffee:

<a href="https://buymeacoffee.com/aaronkwhite" target="_blank"><img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me A Coffee" height="40"></a>

## License

MIT

---

*Wallflower - a quiet reader for a noisy web.*
