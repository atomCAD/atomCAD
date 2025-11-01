## Install Homebrew (Package Manager)

Open Terminal and run:

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

Follow the prompts and add Homebrew to your PATH when instructed.

## Install Git

```bash
brew install git
```

Configure Git with your credentials:

```bash
git config --global user.name "Your Name"
git config --global user.email "your.email@example.com"
```

## Install XCode

Install `Xcode` from the App Store.

After Xcode installation, run:

```bash
# Accept Xcode license
sudo xcodebuild -license accept

# Install additional components
sudo xcodebuild -runFirstLaunch
```

## Install Flutter

```bash
# Install Flutter via Homebrew
brew install --cask flutter

# Add Flutter to your PATH (add this to your ~/.zshrc or ~/.bash_profile)
echo 'export PATH="$PATH:/opt/homebrew/Caskroom/flutter/bin"' >> ~/.zshrc
source ~/.zshrc

# Verify installation
flutter doctor
```

## Install Rust

```bash
# Install Rust via rustup (recommended way)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Follow the prompts (usually just press Enter for defaults)
# Then reload your shell
source ~/.cargo/env

# Verify installation
rustc --version
cargo --version
```

## Clone the AtomCAD Repository

```bash
# Navigate to where you want the project
cd ~/Documents  # or wherever you prefer

# Clone the repository
git clone https://github.com/yourusername/flutter_cad.git  # Replace with actual repo URL
cd flutter_cad
```

## Install Flutter Dependencies

```bash
# Get Flutter dependencies
flutter pub get

# Check if everything is set up correctly
flutter doctor
```

## Step 7: Install Additional Tools (if needed)

If `flutter doctor` shows any issues, you might need:

```bash
# Xcode Command Line Tools (if not already installed)
xcode-select --install

# CocoaPods (for iOS dependencies)
brew install cocoapods
```

## Step 8: Build and Test

```bash
# Build the Rust library
cd rust
cargo build

# Go back to project root and test Flutter
cd ..
flutter run
```

