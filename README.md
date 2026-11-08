<p align="center">
  <img src="src/assets/app-icon.png" alt="Pebble logo" width="120">
</p>

<h1 align="center">Pebble</h1>

<p align="center">
  A local-first desktop email client for people who want a calmer, more private inbox.
</p>

<p align="center">
  <a href="README.zh-CN.md">简体中文</a>
  ·
  <a href="https://github.com/zi-ha/Pebble/releases">Releases</a>
  ·
  <a href="LICENSE">License</a>
</p>

<p align="center">
  <a href="https://github.com/zi-ha/Pebble/releases"><img src="https://img.shields.io/github/v/release/zi-ha/Pebble?style=flat-square&color=d4714e" alt="Release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0-blue?style=flat-square" alt="License"></a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey?style=flat-square" alt="Platform">
</p>

## Overview

Pebble is a desktop mail client built with Rust, Tauri, and React. It keeps mail data, the search index, attachments, rules, and application settings on your device by default.

The app is designed around a few practical ideas:

- Your mailbox should stay readable, fast, and quiet.
- Email workflows should be local-first instead of cloud-dashboard-first.
- Privacy controls should be explicit, visible, and easy to override per message.
- Search, snooze, rules, and a Kanban board should work together instead of living in separate tools.

Pebble currently supports Gmail, IMAP, POP3, and experimental Outlook accounts.

## Highlights

### Local-first privacy

- Local SQLite database for messages, folders, labels, rules, and settings.
- Local Tantivy full-text index for fast search.
- Attachments are stored on disk under the app data directory.
- OAuth tokens and credentials are encrypted with a per-device key.
- No telemetry.
- Network requests are limited to features you configure: mail sync, translation, and optional WebDAV settings backup.

### Mail workflow

- Unified inbox across multiple accounts.
- Gmail, IMAP, POP3, and experimental Outlook support.
- Threaded and message-list views.
- Archive, delete, star, mark read, batch actions, and restore flows.
- Snooze messages and bring them back later.
- Full-text search and advanced filters.
- Rules engine for automatic organization.
- Email CSS rendering with embedded style support.
- Option to allow invalid TLS certificates for self-hosted mail servers.

### Productivity tools

- Local address book with search, favorites, notes, multiple labeled email addresses, and one-click contact actions from message participants.
- Recipient autocomplete that prioritizes saved contacts and falls back to recent correspondents.
- vCard (`.vcf`) import and export for moving contacts between Pebble and other address books. Re-imports merge by normalized email; existing non-empty local names and notes take precedence over imported values.
- Kanban board with Todo, Waiting, and Done columns.
- Command palette and keyboard-first navigation.
- Built-in translation providers with bilingual reading and customizable shortcuts (`T` to translate selection, `Ctrl+Shift+T` to toggle bilingual view).
- Dark and light themes with wallpaper background support.
- English and Chinese UI.
- Optional local file export/import and WebDAV backup for contacts, settings, rules, Kanban cards, Kanban notes, and separately encrypted account secrets.
- Automatic scheduled WebDAV backup with configurable interval.

### Platform integration

- Native macOS traffic-light window controls.
- Register as default email client on Windows (Settings > General).
- Tray mode keeps mail sync running while the window is hidden.
- Start hidden to system tray option.
- `mailto:` protocol handler for composing mail from external apps.

## Screenshots

<table>
  <tr>
    <td><img src="site/screenshots/inbox.png" alt="Inbox"><br><b>Inbox</b></td>
    <td><img src="site/screenshots/kanban.png" alt="Kanban board"><br><b>Kanban</b></td>
  </tr>
  <tr>
    <td><img src="site/screenshots/dark.png" alt="Dark mode"><br><b>Dark Mode</b></td>
    <td><img src="site/screenshots/settings.png" alt="Settings"><br><b>Settings</b></td>
  </tr>
</table>

## Tech Stack

| Layer | Technology |
| --- | --- |
| Desktop shell | Tauri 2 |
| Backend | Rust |
| Frontend | React 19, TypeScript |
| State | Zustand, TanStack Query |
| Database | SQLite via rusqlite |
| Search | Tantivy |
| Styling | Tailwind CSS and app CSS |
| Localization | i18next |

## Getting Started

### Install

Download prebuilt desktop packages from the [Releases](https://github.com/zi-ha/Pebble/releases) page.

On Arch Linux, Pebble is available from the AUR as `pebble-bin`:

```bash
yay -S pebble-bin
# or
paru -S pebble-bin
```

### Prerequisites

- Rust stable
- Node.js 18 or newer
- pnpm 8 or newer
- Tauri system dependencies for your platform

### Profile Storage

By default, Pebble stores the database, search index, attachments, logs, imported backgrounds, and local interface preferences under the system app data directory.

You can opt into a custom or portable profile directory before Pebble starts:

- Set `PEBBLE_PROFILE_DIR` to an absolute path.
- Put `pebble-profile.txt` next to `pebble.exe`; the first line can be an absolute path or a path relative to the executable directory.
- Create a `pebble-profile` directory next to `pebble.exe` to store Pebble data beside the app.

The selected directory is used on the next app launch.

### Development Setup

```bash
git clone https://github.com/zi-ha/Pebble.git
cd Pebble

pnpm install
cp .env.example .env

pnpm dev
```

The development command starts the Vite frontend and the Tauri desktop app.

### Build

```bash
pnpm build:windows
pnpm build:macos
pnpm build:linux
```

Pick the command that matches your platform; there is no platform-agnostic `pnpm build`.
Desktop bundles are written under `target/release/` and `target/release/bundle/`.
On Linux, install the Tauri system dependencies first; `pnpm build:linux` produces AppImage, deb, and rpm packages under `target/release/bundle/`.
macOS bundles are unsigned unless you provide your own signing setup.
After copying an unsigned macOS build to `/Applications`, run the following command before opening it:

```bash
sudo xattr -cr /Applications/Pebble.app
```

### Release

Releases are cut from the app's own version number, never from a hand-typed tag. Bump the version in all three manifests (`package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`) and add a `## [x.y.z] - date` section to `CHANGELOG.md`, then run **Actions → Release (Manual) → Run workflow** on the default branch.

The workflow reads the version from `src-tauri/tauri.conf.json` — the same value the About page reports — refuses to run if the manifests disagree, the changelog has no notes, or a tag for that version already exists, then builds the Windows installer, tags the commit `v<version>` and publishes the GitHub release with that changelog section as its notes.

## OAuth Configuration

Pebble can connect to Gmail and Outlook through OAuth. IMAP accounts use the IMAP/SMTP credentials configured in the app.

Copy `.env.example` to `.env`, then fill the provider values you need.

| Variable | Description |
| --- | --- |
| `GOOGLE_CLIENT_ID` | Google OAuth client ID. Use a Desktop app client when possible. |
| `GOOGLE_CLIENT_SECRET` | Optional for PKCE flows. Add it if Google rejects token exchange with `client_secret is missing`. |
| `MICROSOFT_CLIENT_ID` | Microsoft public/native app client ID. |
| `MICROSOFT_CLIENT_SECRET` | Optional. Leave empty for public/native Microsoft apps. |

## Useful Scripts

| Command | Purpose |
| --- | --- |
| `pnpm dev` | Run the Tauri desktop app in development mode. |
| `pnpm dev:frontend` | Run only the Vite frontend dev server. |
| `pnpm test` | Run frontend tests with Vitest. |
| `pnpm build:frontend` | Type-check and build the frontend. |
| `pnpm build:windows` | Build the Windows NSIS installer. |
| `pnpm build:macos` | Build unsigned macOS `.app` and `.dmg` bundles. |
| `pnpm build:linux` | Build Linux `.AppImage`, `.deb`, and `.rpm` packages. |
| `cargo test -p pebble mail::` | Run the mail module tests. |
| `cargo check` | Check the Rust crate. |

## Project Structure

```text
Pebble/
|-- src/                    React frontend
|   |-- components/         Shared UI components
|   |-- features/           Inbox, compose, search, Kanban, settings
|   |-- hooks/              React hooks and query helpers
|   |-- lib/                IPC API, i18n, utilities
|   `-- stores/             Zustand stores
|-- src-tauri/              Tauri application
|   `-- src/
|       |-- commands/       IPC commands exposed to the frontend
|       |-- core/           Shared types and errors
|       |-- store/          SQLite persistence
|       |-- mail/           Mail providers and sync
|       |-- search/         Tantivy search index
|       |-- crypto/         Credential encryption
|       |-- oauth/          OAuth 2.0 and PKCE
|       |-- rules/          Rules engine
|       |-- translate/      Translation providers
|       `-- privacy/        HTML sanitizing and tracker controls
|-- tests/                  Frontend tests
`-- docs/                   Design notes and plans
```

## Keyboard Shortcuts

| Shortcut | Action |
| --- | --- |
| `J` / `K` | Move through messages |
| `Enter` | Open the selected message |
| `E` | Archive |
| `S` | Toggle star |
| `R` | Reply |
| `A` | Reply all |
| `F` | Forward |
| `C` | Compose |
| `/` | Focus search |
| `T` | Translate selected text |
| `Ctrl+Shift+T` | Toggle bilingual view |
| `Esc` | Close, cancel, or go back |

Shortcuts can be reviewed and customized in Settings.

## Status

Pebble is under active development. It is usable for day-to-day testing, but mail clients handle sensitive data and provider behavior varies. Keep backups of important mail, and verify account actions against your provider when testing new builds.

## Contributing

Issues and pull requests are welcome.

For code changes, please keep patches focused and include tests for behavior changes when practical. Before submitting, run the relevant checks:

```bash
pnpm test
pnpm build:frontend
cargo check
```

## License

Pebble is licensed under the [GNU Affero General Public License v3.0](LICENSE).

---

<p align="center">
  Built by <a href="https://github.com/zi-ha">zi-ha</a>.
  <br>
  Friend link: <a href="https://linux.do">LINUX DO</a>
</p>
