# telerust

A Telegram desktop client built in Rust with a web-based GUI using Tauri 2.

## Features

### ✅ Working
- **Telegram Connection**: Connects to Telegram via MTProto using grammers library
- **Login Flow**: Phone number entry with country selector, code verification
- **Session Persistence**: Login persists across restarts (stored in `%APPDATA%\telerust\telerust\config.toml` + session files)
- **Chat List**: Displays dialogs (chats, groups, channels) with search/filter
- **Messages**: Load and display messages with:
  - Text messages with hyperlink support
  - Images (with lazy loading)
  - Videos (with native controls)
  - Audio/Voice messages (with native controls)
  - Documents/Files (with name and size)
- **Profile Pictures**: Shows avatars for users, groups, channels, supergroups
- **Contact List**: New chat modal with contact search
- **Smooth Animations**: Fade, slide, scale animations throughout
- **Country Selector**: Custom dropdown with:
  - Closed state: `+1 🇺🇸`
  - Open state: `+1 US United States`
  - Smooth slide animations (left/right)
  - Search/filter countries
  - Keyboard navigation
- **Phone Number Formatting**:
  - US/Canada (+1): `555 123 4567` (3-3-4)
  - Other countries: `555 123 456` (3-3-3) then groups of 3
- **Registration Notice**: Banner at top explaining registration must be done via official Telegram app or web.telegram.org
- **Responsive Layout**: Works at different window sizes

### 🐛 Known Bugs / Issues
- **Tauri API Injection**: In `frontendDist` mode (built executable), the Tauri 2 API (`window.__TAURI__`) may not be injected automatically. Workaround: added script loading from `tauri://localhost/__tauri__/tauri.js` with 15s polling fallback.
- **Dev vs Build Config**: The same `tauri.conf.json` is used for both `tauri dev` and `tauri build`. Currently configured for `frontendDist` (built executable). For development with hot reload, you need to temporarily add `devUrl` and `beforeDevCommand` to the config.
- **WebView2 Requirement**: On Windows, requires WebView2 runtime (included in Windows 11, installable on Windows 10)
- **First Run Delay**: Tauri API detection waits up to 15 seconds before showing error

### 📋 TODO
- [ ] Fix Tauri 2 API injection in frontendDist mode properly
- [ ] Add proper dev/prod config separation
- [ ] Add message sending with media (photos, videos, files)
- [ ] Add group/channel management
- [ ] Add settings page (theme, notifications, etc.)
- [ ] Add secret chats support
- [ ] Add push notifications
- [ ] Add multi-account support
- [ ] Improve contact list (show online status, last seen)
- [ ] Add message reactions, replies, forwards
- [ ] Add chat search within messages
- [ ] Add message editing/deletion
- [ ] Add pinned messages
- [ ] Add message scheduling
- [ ] Add dark/light theme toggle
- [ ] Add keyboard shortcuts
- [ ] Add system tray integration
- [ ] Add auto-update support
- [ ] Write unit/integration tests
- [ ] CI/CD pipeline for releases

## Architecture

```
telerust/
├── Cargo.toml                 # Workspace root
├── telerust-core/             # Core Telegram client library
│   ├── src/
│   │   ├── client.rs          # Telegram client wrapper (grammers)
│   │   ├── config.rs          # Configuration management
│   │   ├── types.rs           # Shared types (Dialog, Message, Contact, Photo)
│   │   └── lib.rs
│   └── Cargo.toml
├── telerust-gui-tauri/        # Tauri 2 desktop app
│   ├── frontend/
│   │   └── index.html         # Complete frontend (HTML/CSS/JS)
│   └── src-tauri/
│       ├── src/
│       │   └── main.rs        # Tauri backend with commands
│       ├── Cargo.toml
│       ├── tauri.conf.json
│       ├── build.rs
│       └── icons/
└── telerust-gui-wry/          # Alternative wry-based GUI (legacy)
    ├── src/
    │   └── main.rs
    └── Cargo.toml
```

## Prerequisites

- **Rust** (latest stable): https://rustup.rs/
- **Node.js** (for `tauri dev`): https://nodejs.org/
- **Tauri CLI**: `cargo install tauri-cli --version "^2"`
- **WebView2** (Windows): Included in Windows 11, or install from https://developer.microsoft.com/en-us/microsoft-edge/webview2/
- **Telegram API credentials**: Get from https://my.telegram.org/apps

## Configuration

Create `config.toml` at:
- **Windows**: `%APPDATA%\telerust\telerust\config.toml`
- **Linux**: `~/.config/telerust/telerust/config.toml`
- **macOS**: `~/Library/Application Support/telerust/telerust/config.toml`

```toml
api_id = 12345678
api_hash = "your_api_hash_here"
```

The config file is created automatically on first run if it doesn't exist (you'll need to edit it with your credentials).

## Building

### Development (with hot reload)
```bash
# Temporarily edit tauri.conf.json to add:
# "beforeDevCommand": "npx serve ../frontend -p 1420",
# "devUrl": "http://localhost:1420",

tauri dev
```

### Production Build
```bash
# Current config uses frontendDist (no dev server needed)
cargo build --release -p telerust-gui-tauri

# Or use Tauri CLI for full bundle (NSIS/MSI/.exe installer)
tauri build
```

### Run Built Executable
```bash
.\target\release\telerust-gui-tauri.exe
```

## Project Structure Details

### telerust-core
Core library providing:
- `TelegramClient`: Async wrapper around grammers Client
- `Config`: Handles config file loading/saving with cross-platform paths
- Types: `Dialog`, `Message`, `MessageMedia`, `Contact`, `Photo`, `User`

Key methods:
- `connect()` - Connect to Telegram
- `request_login_code(phone)` - Send login code
- `submit_login_code(code)` - Verify login code
- `get_me()` - Get current user info
- `get_dialogs()` - Get chat list
- `get_contacts()` - Get contact list
- `get_messages(peer_id, limit)` - Get chat history
- `send_message(peer_id, text)` - Send text message
- `download_media(message, media_type)` - Download media files

### telerust-gui-tauri (Tauri 2 Backend)
Rust commands exposed to frontend:
- `connect()` - Initialize Telegram connection
- `request_login_code(phone)` - Request login code
- `submit_login_code(code)` - Submit login code
- `get_me()` - Get current user
- `get_dialogs()` - Get dialogs
- `get_contacts()` - Get contacts
- `get_messages(peer_id, limit)` - Get messages
- `send_message(peer_id, text)` - Send message

### Frontend (index.html)
Single-file frontend with:
- Vanilla HTML/CSS/JS (no build step)
- CSS custom properties for theming
- CSS animations (keyframes + classes)
- Custom country selector component
- Phone number formatting with cursor preservation
- Tauri API initialization with timeout wrapper
- Login → Code verification → Main app flow
- Chat list with search
- Message rendering with media support
- New chat modal with contact search
- Registration notice banner

## Troubleshooting

### "Tauri API not available"
- Make sure you're running via `tauri dev` (development) or the built executable from `tauri build` / `cargo build --release`
- The app waits up to 15 seconds for API injection before showing error

### "Config file not found"
- Run the app once to create the config directory
- Edit the generated `config.toml` with your API credentials
- Restart the app

### WebView2 errors (Windows)
- Install WebView2 runtime: https://developer.microsoft.com/en-us/microsoft-edge/webview2/
- Or update Windows to latest version

### Connection issues
- Check internet connection
- Verify API credentials in config.toml
- Telegram may rate-limit new logins - wait a bit and retry

## License

MIT License - feel free to use and modify.

## Credits

- [grammers](https://github.com/LonamiWebs/grammers) - Telegram MTProto library
- [Tauri](https://tauri.app/) - Desktop app framework
- [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) - Windows webview