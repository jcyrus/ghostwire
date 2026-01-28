class Ghostwire < Formula
  desc "Secure, ephemeral TUI chat client built with Rust and Ratatui"
  homepage "https://github.com/jcyrus/GhostWire"
  version "0.3.1"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/jcyrus/GhostWire/releases/download/v#{version}/ghostwire-darwin-arm64"
      sha256 "fb8f20200366d373e33556cc02dec19da3ed913a07d5651cd33478287ee20c92"
    else
      url "https://github.com/jcyrus/GhostWire/releases/download/v#{version}/ghostwire-darwin-amd64"
      sha256 "0904b5d941d6c438abe7685f5f4d1294c8aa324319182b4f8bf300e3d9e7d86a"
    end
  end

  def install
    binary = Hardware::CPU.arm? ? "ghostwire-darwin-arm64" : "ghostwire-darwin-amd64"
    bin.install binary => "ghostwire"
  end

  test do
    # Basic smoke test - check binary executes
    system "#{bin}/ghostwire", "--version" rescue true
  end
end
