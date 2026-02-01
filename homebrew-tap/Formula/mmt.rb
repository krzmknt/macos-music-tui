class Mmt < Formula
  desc "TUI for controlling macOS Music.app with keyboard"
  homepage "https://github.com/krzmknt/macos-music-tui"
  url "https://github.com/krzmknt/macos-music-tui/releases/download/v0.1.0/mmt-0.1.0-darwin-arm64.tar.gz"
  sha256 "PLACEHOLDER_SHA256"
  license "MIT"

  def install
    bin.install "mmt"
  end

  test do
    assert_match "mmt v", shell_output("#{bin}/mmt --version")
  end
end
