class PactrunExact100Alpha4 < Formula
  desc 'Pactrun immutable revisions and managed instances'
  homepage 'https://github.com/Doner357/pactrun'
  license 'MIT'
  url 'https://github.com/Doner357/pactrun/releases/download/v1.0.0-alpha.4/pactrun-1.0.0-alpha.4-linux-x86_64.tar.gz'
  version '1.0.0-alpha.4'
  sha256 'dbd0ebf467712e77df9266f4db0ec8c4c84b94635a30a54595ab7cf0298f5e87'
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
