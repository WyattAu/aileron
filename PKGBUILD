# Maintainer: Wyatt Au <wyatt@patch.com>
pkgname=aileron
pkgver=0.8.0
pkgrel=1
pkgdesc="Keyboard-driven tiling web environment for developers"
arch=('x86_64')
url="https://github.com/WyattAu/aileron"
license=('MIT')
depends=('webkit2gtk-4.1' 'gtk3' 'openssl' 'sqlite' 'vulkan-driver' 'wayland')
makedepends=('cargo' 'clang' 'pkgconf')
provides=('aileron')
conflicts=('aileron-git' 'aileron-bin')
options=('!lto')
source=("git+https://github.com/WyattAu/aileron.git#tag=v${pkgver}")
sha256sums=('SKIP')

prepare() {
    cd "$srcdir/aileron"
    export RUSTUP_TOOLCHAIN=stable
    cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
    cd "$srcdir/aileron"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --frozen --release
}

check() {
    cd "$srcdir/aileron"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo test --frozen --lib
}

package() {
    cd "$srcdir/aileron"
    install -Dm755 target/release/aileron "$pkgdir/usr/bin/aileron"
    install -Dm644 resources/aileron.svg "$pkgdir/usr/share/icons/hicolor/scalable/apps/aileron.svg"
    install -Dm644 resources/aileron.desktop "$pkgdir/usr/share/applications/aileron.desktop"
    install -Dm644 README.md "$pkgdir/usr/share/doc/aileron/README.md"
    install -Dm644 CHANGELOG.md "$pkgdir/usr/share/doc/aileron/CHANGELOG.md"
    install -Dm644 CONTRIBUTING.md "$pkgdir/usr/share/doc/aileron/CONTRIBUTING.md"
}
