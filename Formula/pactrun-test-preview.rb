class PactrunTestPreview < Formula
  desc 'Pactrun immutable revisions and managed instances'
  homepage 'https://github.com/Doner357/pactrun'
  license 'MIT'
  url 'https://github.com/Doner357/pactrun/releases/download/v1.0.0-alpha.8/pactrun-test-1.0.0-alpha.8-linux-x86_64.tar.gz'
  version '1.0.0-alpha.8'
  sha256 '23279692fb1e56bb440815110494378913cf89ca6c574ab73cbb7967038ea601'
  depends_on :linux
  depends_on arch: :x86_64
  def install
    Formula.installed.each do |other|
      if other.name != name && (other.opt_bin/"pactrun-test").exist?
        raise "Uninstall #{other.full_name} before installing #{full_name}; managed data is retained."
      end
    end
    prefix.install 'bin', 'LICENSE', 'THIRD_PARTY_NOTICES.txt', 'rust-licenses'
  end
  test do
    assert_match version.to_s, shell_output("#{bin}/pactrun-test --version")
  end
end
