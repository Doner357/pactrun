class PactrunTestPreview < Formula
  desc 'Pactrun immutable revisions and managed instances'
  homepage 'https://github.com/Pipapoen/pactrun'
  license 'MIT'
  url 'https://github.com/Pipapoen/pactrun/releases/download/v1.0.0-alpha.9/pactrun-test-1.0.0-alpha.9-linux-x86_64.tar.gz'
  version '1.0.0-alpha.9'
  sha256 '2b7f1a32affa0fd8108397968575cb776cd62c5251356d17831691824d471b59'
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
