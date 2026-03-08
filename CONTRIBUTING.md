# Contributing to GhostWire

Thank you for your interest in contributing to GhostWire! We welcome contributions from the community.

## 🚀 Getting Started

### Prerequisites

- Rust 1.70+ (2021 Edition)
- Git
- A terminal with TrueColor support

### Setup

```bash
git clone https://github.com/jcyrus/GhostWire.git
cd GhostWire
cargo build
```

## 📋 How to Contribute

### Reporting Bugs

- Use the GitHub issue tracker
- Include steps to reproduce
- Provide system information (OS, Rust version)
- Include relevant logs or screenshots

### Suggesting Features

- Open an issue with the `enhancement` label
- Describe the use case
- Explain how it fits with GhostWire's philosophy

### Pull Requests

1. **Fork the repository**
2. **Create a feature branch**

   ```bash
   git checkout -b feature/your-feature-name
   ```

3. **Make your changes**
   - Follow the code style (run `cargo fmt`)
   - Add tests if applicable
   - Update documentation

4. **Test your changes**

   ```bash
   cargo test
   cargo clippy -- -D warnings
   cargo build --release
   ```

5. **Commit with conventional commits**

   ```bash
   git commit -m "feat(client): add new feature"
   ```

   Types: `feat`, `fix`, `docs`, `chore`, `refactor`, `test`

6. **Push and create PR**
   ```bash
   git push origin feature/your-feature-name
   ```

## 🎨 Code Style

- **Formatting**: Use `cargo fmt` (rustfmt)
- **Linting**: Pass `cargo clippy -- -D warnings`
- **Naming**: Follow Rust conventions
  - `snake_case` for functions and variables
  - `PascalCase` for types and traits
  - `SCREAMING_SNAKE_CASE` for constants

## 🏗️ Architecture

- **Client**: `client/src/`
  - `main.rs` - Entry point and UI loop
  - `app.rs` - Application state
  - `network.rs` - WebSocket communication
  - `ui.rs` - Ratatui rendering

- **Server**: `server/src/`
  - `main.rs` - Shuttle entry point
  - `local.rs` - Local development entry
  - `relay.rs` - Core relay logic

## 📝 Commit Guidelines

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

**Examples:**

```
feat(channels): add group channel support
fix(ui): resolve scroll position bug
docs(readme): update installation instructions
chore(deps): update ratatui to 0.26
```

## 🧪 Testing

```bash
# Run all tests
cargo test

# Test specific package
cargo test -p ghostwire-client

# Run with output
cargo test -- --nocapture
```

## 📚 Documentation

- Update relevant `.md` files
- Add TSDoc comments for public APIs
- Update `CHANGELOG.md` under `[Unreleased]`

## ⚖️ License

By contributing, you agree that your contributions will be licensed under the MIT License.

## 💬 Questions?

- Open a discussion on GitHub
- Check existing issues and PRs
- Read the [developer documentation](docs/dev/) for architecture and implementation details

---

**Thank you for contributing to GhostWire! 👻**
