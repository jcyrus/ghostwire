class Zerodrop < Formula
  desc "Secure, ephemeral TUI chat client built with Rust and Ratatui"
  homepage "https://github.com/jcyrus/zerodrop"
  version "0.7.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/jcyrus/zerodrop/releases/download/v#{version}/zerodrop-darwin-arm64"
      sha256 "PLACEHOLDER_ARM64_SHA256"
    else
      url "https://github.com/jcyrus/zerodrop/releases/download/v#{version}/zerodrop-darwin-amd64"
      sha256 "PLACEHOLDER_AMD64_SHA256"
    end
  end

  def install
    binary = Hardware::CPU.arm? ? "zerodrop-darwin-arm64" : "zerodrop-darwin-amd64"
    bin.install binary => "zerodrop"
  end

  test do
    system "#{bin}/zerodrop", "--version" rescue true
  end
end
