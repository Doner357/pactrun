class PactrunTestExact100Alpha3 < Formula
  desc 'Pactrun immutable revisions and managed instances'
  homepage 'https://github.com/Doner357/pactrun'
  license 'MIT'
  url 'https://github.com/Doner357/pactrun/releases/download/v1.0.0-alpha.3/pactrun-test-1.0.0-alpha.3-linux-x86_64.tar.gz'
  version '1.0.0-alpha.3'
  sha256 '1bbff055c67b70d25c3b0a38dd0fbf0145363acb1e9978caba100d43490bab76'
  depends_on :linux
  depends_on arch: :x86_64
  def install
    Formula.installed.each do |other|
      if other.name != name && (other.opt_bin/"pactrun-test").exist?
        raise "Uninstall #{other.full_name} before installing #{full_name}; managed data is retained."
      end
    end
    prefix.install 'bin', 'libexec', 'LICENSE', 'THIRD_PARTY_NOTICES.txt', 'rust-licenses'
  end
  test do
    assert_match version.to_s, shell_output("#{bin}/pactrun-test --version")
  end
end
