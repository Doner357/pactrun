class PactrunExact100Alpha3 < Formula
  desc 'Pactrun immutable revisions and managed instances'
  homepage 'https://github.com/Doner357/pactrun'
  license 'MIT'
  url 'https://github.com/Doner357/pactrun/releases/download/v1.0.0-alpha.3/pactrun-1.0.0-alpha.3-linux-x86_64.tar.gz'
  version '1.0.0-alpha.3'
  sha256 '458e863385984e7205cba384b41bcab2c778afe0c84a1e50ca160d9b06782255'
  depends_on :linux
  depends_on arch: :x86_64
  def install
    Formula.installed.each do |other|
      if other.name != name && (other.opt_bin/"pactrun").exist?
        raise "Uninstall #{other.full_name} before installing #{full_name}; managed data is retained."
      end
    end
    prefix.install 'bin', 'libexec', 'LICENSE', 'THIRD_PARTY_NOTICES.txt', 'rust-licenses'
  end
  test do
    assert_match version.to_s, shell_output("#{bin}/pactrun --version")
  end
end
