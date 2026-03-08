# GhostWire Documentation

## 👤 User Documentation — [`docs/user/`](user/)

Guides for installing, configuring, and using GhostWire.

| Document                           | Description                                                                         |
| ---------------------------------- | ----------------------------------------------------------------------------------- |
| [**User Guide**](user/GUIDE.md)    | Comprehensive guide — interface, keybinds, commands, configuration, troubleshooting |
| [Security Model](user/SECURITY.md) | Cryptographic stack, threat model, what GhostWire protects                          |
| [Windows Install](user/WINDOWS.md) | Windows-specific installation and PATH setup                                        |

## 🛠 Developer Documentation — [`docs/dev/`](dev/)

Architecture and implementation details for contributors.

| Document                              | Description                                      |
| ------------------------------------- | ------------------------------------------------ |
| [Client Architecture](dev/CLIENT.md)  | Async/sync split, thread model, module breakdown |
| [Server Architecture](dev/SERVER.md)  | Relay pattern, state management, deployment      |
| [Feature Details](dev/FEATURES.md)    | Implementation specifics by version              |
| [Local Development](dev/LOCAL_DEV.md) | Two-binary dev setup, testing procedures         |

## 🚀 Quick Links

| I want to...                    | Read this                            |
| ------------------------------- | ------------------------------------ |
| Install and use GhostWire       | [Quick Start](../QUICKSTART.md)      |
| Learn all features and keybinds | [User Guide](user/GUIDE.md)          |
| Understand channels and DMs     | [Channels](../CHANNELS.md)           |
| Contribute code                 | [Contributing](../CONTRIBUTING.md)   |
| See what's new                  | [Changelog](../CHANGELOG.md)         |
| Understand the security model   | [Security](user/SECURITY.md)         |
| Deploy the server               | [Server Architecture](dev/SERVER.md) |
| Debug local development         | [Local Dev](dev/LOCAL_DEV.md)        |

## 📁 Structure

```
docs/
├── README.md              # This file — documentation hub
├── user/                  # User-facing documentation
│   ├── README.md          # User docs index
│   ├── GUIDE.md           # Comprehensive user guide
│   ├── SECURITY.md        # Security model & threat analysis
│   └── WINDOWS.md         # Windows installation guide
└── dev/                   # Developer documentation
    ├── README.md          # Dev docs index
    ├── CLIENT.md          # Client architecture
    ├── SERVER.md          # Server architecture
    ├── FEATURES.md        # Feature implementation details
    └── LOCAL_DEV.md       # Local development setup
```

---

**Built with 🦀 and ☕ by jCyrus**
