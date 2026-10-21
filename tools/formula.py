#!/usr/bin/env python3
"""Print the Homebrew formula for a published release.

usage: tools/formula.py <version> <checksum directory>
The directory holds the release's tonic<version>.<platform>.tar.sha256 files.
"""
from pathlib import Path
import sys

TEMPLATE = '''class Tonic < Formula
  desc "Elixir-to-native compiler with an LLVM backend and process runtime"
  homepage "https://wess.io/tonic/"
  version "{version}"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/wess/tonic/releases/download/v{version}/tonic{version}.macosarm64.tar.gz"
      sha256 "{macos}"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/wess/tonic/releases/download/v{version}/tonic{version}.linuxx64.tar.gz"
      sha256 "{linux}"
    end
  end

  on_macos do
    depends_on arch: :arm64
    depends_on macos: :sonoma
  end

  on_linux do
    depends_on arch: :x86_64
    depends_on "llvm"
  end

  def install
    libexec.install "bin", "lib", "share"
    pkgshare.install "hello.exs"
    doc.install "README.md", "docs", "licenses", "notice", "manifest.json"
    # compiles and links with the Xcode Command Line Tools clang on macOS
    env = {{}}
    env[:TONIC_CC] = "${{TONIC_CC:-#{{Formula["llvm"].opt_bin}}/clang}}" if OS.linux?
    (bin/"tonic").write_env_script libexec/"bin/tonic", env
  end

  def caveats
    <<~EOS
      For the Mix tasks (mix tonic build|run|check):
        mix archive.install #{{opt_libexec}}/share/tonic/tonic.ez --force
    EOS
  end

  test do
    assert_match "tonic #{{version}}", shell_output("#{{bin}}/tonic --version")
    system bin/"tonic", "build", pkgshare/"hello.exs", "-o", testpath/"hello"
    assert_equal "Hello, world!", shell_output(testpath/"hello").strip
  end
end
'''


def checksum(directory, version, platform):
    text = (directory / f"tonic{version}.{platform}.tar.sha256").read_text()
    value = text.split()[0]
    assert len(value) == 64, f"bad checksum for {platform}"
    return value


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit(__doc__.strip())
    version = sys.argv[1].removeprefix("v")
    directory = Path(sys.argv[2])
    sys.stdout.write(TEMPLATE.format(version=version,
                                     macos=checksum(directory, version, "macosarm64"),
                                     linux=checksum(directory, version, "linuxx64")))
