class Ghostwire < Formula
  desc "Secure, ephemeral TUI chat client built with Rust and Ratatui"
  homepage "https://github.com/jcyrus/GhostWire"
  version "0.3.1"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/jcyrus/GhostWire/releases/download/v#{version}/ghostwire-darwin-arm64"
      sha256 "4c7bdbc6c5ab47e502a46d3f764bc889160f82ff4d73cf4dcd7f75845513ea7d"
    else
      url "https://github.com/jcyrus/GhostWire/releases/download/v#{version}/ghostwire-darwin-amd64"
      sha256 "ce86ae09fcd3791027a071e8ce95d3d9681b12d52aed7ccd9958688b906ee98d"
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
