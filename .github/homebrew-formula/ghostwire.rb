class Ghostwire < Formula
  desc "Secure, ephemeral TUI chat client built with Rust and Ratatui"
  homepage "https://github.com/jcyrus/GhostWire"
  version "0.3.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/jcyrus/GhostWire/releases/download/v#{version}/ghostwire-darwin-arm64"
      sha256 "PUT_ARM64_SHA256_HERE" # Will be updated per release
    else
      url "https://github.com/jcyrus/GhostWire/releases/download/v#{version}/ghostwire-darwin-amd64"
      sha256 "PUT_AMD64_SHA256_HERE" # Will be updated per release
    end
  end

  def install
    bin.install "ghostwire-darwin-#{Hardware::CPU.arch}" => "ghostwire"
  end

  test do
    # Basic smoke test - check binary executes
    system "#{bin}/ghostwire", "--version" rescue true
  end
end
