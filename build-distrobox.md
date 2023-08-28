# Containerized Linux Build Environment with Distrobox

The standard AtomCAD build instructions for Linux involve installing the C/C++ dependencies with your system's package manager.  Sometimes this is not an option, whether because your employer requires you to use a distro that's too old, like RHEL or an old Ubuntu LTS, or your distro has an immutable base and will become fragile with that many packages layered on top of it, like Fedora Silverblue.  For server software, Docker is the standard Linux container platform, but Docker doesn't have the GUI or home filesystem integration needed for a dev container for GUI software like AtomCAD.  That's where Distrobox comes in.  Distrobox is a wrapper around [Podman](https://podman.io), which keeps the OS packages separate while preconfiguring integration of X11, Wayland, PulseAudio, your home filesystem, and your user's username/uid/gid.  This isn't particularly secure, but it does keep your base OS cleaner.  Docker is still recommended for headless CI/CD.

## Installing the Tools

1. Install Rust to your home directory with [RustUp](https://rustup.rs).

2. Install [Distrobox](https://github.com/89luca89/distrobox).

## Creating the Container

```
user@host $ distrobox create --name atomcad-dev --image $distro_docker_image:$tag
```

NOTE: if your system uses an Nvidia GPU, add the `--nvidia` flag to `distrobox create`.

## Setting up the Environment

```
user@host $ distrobox enter atomcad-dev
# Ubuntu
user@atomcad-dev $ sudo apt install build-essential cmake libx11-dev
# Fedora
user@atomcad-dev $ sudo dnf groupinstall "Development Tools" && sudo dnf install gcc-c++ cmake libX11-devel
```

## Start Developing

Once your dependencies are installed, open up AtomCAD in your editor, run `distrobox enter atomcad-dev` in your preferred terminal, and invoke `cargo run`.  The AtomCAD window should open at the end of the build as usual.
