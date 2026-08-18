class CliNer < Formula
  desc "Advanced, safe, and documented CLI for macOS disk space management and cleanup"
  homepage "https://github.com/fabrizioriccardo73/cli-ner"
  url "https://github.com/fabrizioriccardo73/cli-ner/archive/refs/tags/v0.1.1.tar.gz"
  sha256 "REPLACE_WITH_SOURCE_OR_RELEASE_SHA256"
  license "MIT"
  head "https://github.com/fabrizioriccardo73/cli-ner.git", branch: "master"

  depends_on "rust" => :build
  depends_on :macos

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "cli-ner", shell_output("#{bin}/cli-ner --version")
    assert_match "System Health Check", shell_output("#{bin}/cli-ner doctor")
  end
end
