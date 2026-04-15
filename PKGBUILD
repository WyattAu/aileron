# Maintainer: Wyatt Au <wyatt@patch.com>
pkgname=aileron-bin
pkgver=0.1.0
pkgrel=1
pkgdesc="Keyboard-driven tiling web environment for developers"
arch=('x86_64')
url="https://github.com/WyattAu/aileron"
license=('MIT')
depends=('webkit2gtk-4.1' 'gtk3' 'openssl' 'sqlite' 'libxdo' 'vulkan-driver' 'wayland')
makedepends=('nix')
options=('!debug' '!strip')
source=("git+https://github.com/WyattAu/aileron.git#tag=v${pkgver}")
sha256sums=('SKIP')

build() {
    cd "$srcdir/aileron"
    nix build
}

package() {
    cd "$srcdir/aileron"
    install -Dm755 result/bin/aileron "$pkgdir/usr/bin/aileron"
    install -Dm644 README.md "$pkgdir/usr/share/doc/aileron/README.md"
}
