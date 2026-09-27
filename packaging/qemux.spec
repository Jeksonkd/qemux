Name:           qemux
Version:        0.1.3
Release:        1%{?dist}
Summary:        A lightweight GTK4 manager for QEMU virtual machines

License:        MIT
URL:            https://github.com/Jeksonkd/qemux
Source0:        https://github.com/Jeksonkd/qemux/archive/refs/tags/v%{version}.tar.gz#/%{name}-%{version}.tar.gz

BuildRequires:  rust
BuildRequires:  cargo
BuildRequires:  gtk4-devel
BuildRequires:  pkgconf-pkg-config
BuildRequires:  desktop-file-utils
BuildRequires:  libappstream-glib

Requires:       gtk4
Requires:       qemu-system-x86
Requires:       qemu-img

%description
Qemux is a desktop app for creating, editing, launching and deleting
QEMU virtual machines. It provides CPU/RAM/storage sliders, a virtual
disk browser that shows which disks are in use versus free (with the
option to reuse an unused disk instead of creating a new one),
automatic OS icon detection from the chosen ISO's file name, and
keyboard shortcuts for common actions.

%prep
%autosetup -n %{name}-%{version}

%build
cargo build --release

%install
install -Dm0755 target/release/qemux %{buildroot}%{_bindir}/qemux
install -Dm0644 flatpak/io.github.Jeksonkd.Qemux.desktop \
    %{buildroot}%{_datadir}/applications/io.github.Jeksonkd.Qemux.desktop
install -Dm0644 flatpak/io.github.Jeksonkd.Qemux.metainfo.xml \
    %{buildroot}%{_metainfodir}/io.github.Jeksonkd.Qemux.metainfo.xml
install -Dm0644 flatpak/io.github.Jeksonkd.Qemux.svg \
    %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/io.github.Jeksonkd.Qemux.svg

desktop-file-validate %{buildroot}%{_datadir}/applications/io.github.Jeksonkd.Qemux.desktop
appstream-util validate-relax --nonet \
    %{buildroot}%{_metainfodir}/io.github.Jeksonkd.Qemux.metainfo.xml

%files
%license LICENSE
%{_bindir}/qemux
%{_datadir}/applications/io.github.Jeksonkd.Qemux.desktop
%{_metainfodir}/io.github.Jeksonkd.Qemux.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/io.github.Jeksonkd.Qemux.svg

%changelog
* Sun Sep 27 2026 Jeksonkd - 0.1.3-1
- Initial package
