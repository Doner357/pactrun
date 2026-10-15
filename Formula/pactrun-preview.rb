class PactrunPreview < Formula
  desc 'Pactrun immutable revisions and managed instances'
  homepage 'https://github.com/Pipapoen/pactrun'
  license 'MIT'
  url 'https://github.com/Pipapoen/pactrun/releases/download/v1.0.0-alpha.9/pactrun-1.0.0-alpha.9-linux-x86_64.tar.gz'
  version '1.0.0-alpha.9'
  sha256 '2b44868ce2c34cfc1ae5e397e58b29ea963674a95bd7a41cd4cdcf0baec42b5d'
  depends_on :linux
  depends_on arch: :x86_64
  def install
    Formula.installed.each do |other|
      if other.name != name && (other.opt_bin/"pactrun").exist?
        raise "Uninstall #{other.full_name} before installing #{full_name}; managed data is retained."
      end
    end
    prefix.install 'bin', 'LICENSE', 'THIRD_PARTY_NOTICES.txt', 'rust-licenses'
  end
  test do
    assert_match version.to_s, shell_output("#{bin}/pactrun --version")
  end
end
